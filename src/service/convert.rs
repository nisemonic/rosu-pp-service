use std::sync::Arc;
use std::time::Instant;

use rosu_mods::GameModsIntermode;
use rosu_pp::{
    any::{DifficultyAttributes, HitResultPriority, PerformanceAttributes, ScoreState, Strains},
    catch::CatchDifficultyAttributes, catch::CatchPerformanceAttributes,
    mania::ManiaDifficultyAttributes, mania::ManiaPerformanceAttributes,
    model::mode::GameMode,
    osu::OsuDifficultyAttributes, osu::OsuPerformanceAttributes,
    taiko::TaikoDifficultyAttributes, taiko::TaikoPerformanceAttributes,
    Beatmap, Difficulty, Performance,
};
use tracing::{debug, trace, warn};

use crate::error::{Error, Result};
use crate::proto::{self, DifficultySettings, ScoreParams};

pub fn parse_beatmap(source: &proto::BeatmapSource) -> Result<Arc<Beatmap>> {
    match &source.source {
        Some(proto::beatmap_source::Source::Content(bytes)) => {
            parse_beatmap_cached(bytes)
        }
        Some(proto::beatmap_source::Source::Path(path)) => {
            let bytes = std::fs::read(path).map_err(|e| Error::Parse(e.into()))?;
            parse_beatmap_cached(&bytes)
        }
        None => Err(Error::MissingBeatmap),
    }
}

fn parse_beatmap_cached(bytes: &[u8]) -> Result<Arc<Beatmap>> {
    if let Some(id) = crate::cache::parse_beatmap_id(bytes) {
        if let Some(map) = crate::cache::get(id) {
            return Ok(map);
        }
        let start = Instant::now();
        let map = Beatmap::from_bytes(bytes).map_err(Error::from)?;
        let n_objects = map.hit_objects.len();
        crate::cache::insert(id, map);
        debug!(beatmap_id = id, objects = n_objects, elapsed_us = start.elapsed().as_micros(), "parsed beatmap");
        crate::cache::get(id).ok_or(Error::MissingBeatmap)
    } else {
        let start = Instant::now();
        let map = Beatmap::from_bytes(bytes).map_err(Error::from)?;
        trace!(objects = map.hit_objects.len(), elapsed_us = start.elapsed().as_micros(), "parsed uncacheable beatmap (no id)");
        Ok(Arc::new(map))
    }
}

pub fn proto_to_mode(mode: proto::GameMode) -> Option<GameMode> {
    match mode {
        proto::GameMode::Unspecified => None,
        proto::GameMode::Osu => Some(GameMode::Osu),
        proto::GameMode::Taiko => Some(GameMode::Taiko),
        proto::GameMode::Catch => Some(GameMode::Catch),
        proto::GameMode::Mania => Some(GameMode::Mania),
    }
}

pub fn proto_to_priority(priority: proto::HitResultPriority) -> HitResultPriority {
    match priority {
        proto::HitResultPriority::BestCase | proto::HitResultPriority::Fastest => HitResultPriority::BestCase,
        proto::HitResultPriority::WorstCase => HitResultPriority::WorstCase,
    }
}

pub fn mode_to_proto(mode: GameMode) -> proto::GameMode {
    match mode {
        GameMode::Osu => proto::GameMode::Osu,
        GameMode::Taiko => proto::GameMode::Taiko,
        GameMode::Catch => proto::GameMode::Catch,
        GameMode::Mania => proto::GameMode::Mania,
    }
}

pub fn apply_difficulty_settings(mut d: Difficulty, s: &DifficultySettings) -> Difficulty {
    // Prefer mods_str (acronym-based) over mods (bitflags) for lazer mod support (CL, etc.)
    if let Some(ref mods_str) = s.mods_str {
        match mods_str.parse::<GameModsIntermode>() {
            Ok(game_mods) => {
                d = d.mods(game_mods);
                debug!("Parsed mods_str '{}' successfully", mods_str);
            }
            Err(e) => {
                warn!("Failed to parse mods_str '{}': {:?}, falling back to bitflags", mods_str, e);
                if let Some(v) = s.mods { d = d.mods(v); }
            }
        }
    } else if let Some(v) = s.mods {
        d = d.mods(v);
    }
    if let Some(v) = s.clock_rate { d = d.clock_rate(v); }
    if let Some(v) = s.ar { d = d.ar(v, s.ar_with_mods.unwrap_or(false)); }
    if let Some(v) = s.cs { d = d.cs(v, s.cs_with_mods.unwrap_or(false)); }
    if let Some(v) = s.hp { d = d.hp(v, s.hp_with_mods.unwrap_or(false)); }
    if let Some(v) = s.od { d = d.od(v, s.od_with_mods.unwrap_or(false)); }
    if let Some(v) = s.passed_objects { d = d.passed_objects(v); }
    if let Some(v) = s.hardrock_offsets { d = d.hardrock_offsets(v); }
    if let Some(v) = s.lazer { d = d.lazer(v); }
    d
}

pub fn apply_score_params<'a>(mut p: Performance<'a>, s: &ScoreParams) -> Performance<'a> {
    if let Some(v) = s.combo { p = p.combo(v); }
    if let Some(v) = s.accuracy { p = p.accuracy(v); }
    if let Some(v) = s.misses { p = p.misses(v); }
    if let Some(v) = s.n300 { p = p.n300(v); }
    if let Some(v) = s.n100 { p = p.n100(v); }
    if let Some(v) = s.n50 { p = p.n50(v); }
    if let Some(v) = s.n_geki { p = p.n_geki(v); }
    if let Some(v) = s.n_katu { p = p.n_katu(v); }
    if let Some(v) = s.large_tick_hits { p = p.large_tick_hits(v); }
    if let Some(v) = s.small_tick_hits { p = p.small_tick_hits(v); }
    if let Some(v) = s.slider_end_hits { p = p.slider_end_hits(v); }
    if let Some(v) = s.hitresult_priority {
        if v != proto::HitResultPriority::BestCase as i32 {
            p = p.hitresult_priority(proto_to_priority(s.hitresult_priority()));
        }
    }
    p
}

/// Calculate effective CS with mods and custom settings applied.
/// If custom_cs is set with cs_with_mods=true, returns it as-is.
/// If custom_cs is set without cs_with_mods, applies mods to it.
/// Otherwise applies mods to base_cs.
/// HR: CS * 1.3 (max 10), EZ: CS * 0.5
pub fn effective_cs(base_cs: f32, settings: Option<&DifficultySettings>) -> f64 {
    const EZ: u32 = 2;
    const HR: u32 = 16;

    let (cs, with_mods) = settings
        .and_then(|s| s.cs.map(|cs| (f64::from(cs), s.cs_with_mods.unwrap_or(false))))
        .unwrap_or((f64::from(base_cs), false));

    if with_mods {
        return cs;
    }

    let mods = settings.and_then(|s| s.mods).unwrap_or(0);

    if mods & HR != 0 {
        (cs * 1.3).min(10.0)
    } else if mods & EZ != 0 {
        cs * 0.5
    } else {
        cs
    }
}

pub fn difficulty_to_proto(attrs: DifficultyAttributes, cs: f64) -> proto::DifficultyResponse {
    let mode = match &attrs {
        DifficultyAttributes::Osu(_) => proto::GameMode::Osu,
        DifficultyAttributes::Taiko(_) => proto::GameMode::Taiko,
        DifficultyAttributes::Catch(_) => proto::GameMode::Catch,
        DifficultyAttributes::Mania(_) => proto::GameMode::Mania,
    };

    proto::DifficultyResponse {
        mode: mode.into(),
        stars: attrs.stars(),
        max_combo: attrs.max_combo(),
        attributes: Some(match attrs {
            DifficultyAttributes::Osu(a) => proto::difficulty_response::Attributes::Osu(osu_diff(a, cs)),
            DifficultyAttributes::Taiko(a) => proto::difficulty_response::Attributes::Taiko(taiko_diff(a)),
            DifficultyAttributes::Catch(a) => proto::difficulty_response::Attributes::Catch(catch_diff(a)),
            DifficultyAttributes::Mania(a) => proto::difficulty_response::Attributes::Mania(mania_diff(a)),
        }),
    }
}

pub fn performance_to_proto(attrs: PerformanceAttributes, cs: f64) -> proto::PerformanceResponse {
    let mode = match &attrs {
        PerformanceAttributes::Osu(_) => proto::GameMode::Osu,
        PerformanceAttributes::Taiko(_) => proto::GameMode::Taiko,
        PerformanceAttributes::Catch(_) => proto::GameMode::Catch,
        PerformanceAttributes::Mania(_) => proto::GameMode::Mania,
    };

    proto::PerformanceResponse {
        mode: mode.into(),
        pp: attrs.pp(),
        stars: attrs.stars(),
        max_combo: attrs.max_combo(),
        attributes: Some(match attrs {
            PerformanceAttributes::Osu(a) => proto::performance_response::Attributes::Osu(osu_perf(a, cs)),
            PerformanceAttributes::Taiko(a) => proto::performance_response::Attributes::Taiko(taiko_perf(a)),
            PerformanceAttributes::Catch(a) => proto::performance_response::Attributes::Catch(catch_perf(a)),
            PerformanceAttributes::Mania(a) => proto::performance_response::Attributes::Mania(mania_perf(a)),
        }),
    }
}

pub fn strains_to_proto(strains: Strains) -> proto::StrainsResponse {
    let section_length = strains.section_len();
    let (mode, data) = match strains {
        Strains::Osu(s) => (proto::GameMode::Osu, proto::strains_response::Strains::Osu(proto::OsuStrains {
            aim: s.aim,
            aim_no_sliders: s.aim_no_sliders,
            speed: s.speed,
            flashlight: s.flashlight,
        })),
        Strains::Taiko(s) => (proto::GameMode::Taiko, proto::strains_response::Strains::Taiko(proto::TaikoStrains {
            color: s.color,
            reading: s.reading,
            rhythm: s.rhythm,
            stamina: s.stamina,
            single_color_stamina: s.single_color_stamina,
        })),
        Strains::Catch(s) => (proto::GameMode::Catch, proto::strains_response::Strains::Catch(proto::CatchStrains {
            movement: s.movement,
        })),
        Strains::Mania(s) => (proto::GameMode::Mania, proto::strains_response::Strains::Mania(proto::ManiaStrains {
            strains: s.strains,
        })),
    };

    proto::StrainsResponse {
        mode: mode.into(),
        section_length,
        strains: Some(data),
    }
}

fn osu_diff(a: OsuDifficultyAttributes, cs: f64) -> proto::OsuDifficultyAttributes {
    proto::OsuDifficultyAttributes {
        aim: a.aim,
        speed: a.speed,
        flashlight: a.flashlight,
        slider_factor: a.slider_factor,
        speed_note_count: a.speed_note_count,
        aim_difficult_strain_count: a.aim_difficult_strain_count,
        speed_difficult_strain_count: a.speed_difficult_strain_count,
        aim_difficult_slider_count: a.aim_difficult_slider_count,
        ar: a.ar,
        od: a.od(),
        hp: a.hp,
        cs,
        great_hit_window: a.great_hit_window,
        ok_hit_window: a.ok_hit_window,
        meh_hit_window: a.meh_hit_window,
        n_circles: a.n_circles,
        n_sliders: a.n_sliders,
        n_spinners: a.n_spinners,
        n_large_ticks: a.n_large_ticks,
    }
}

fn taiko_diff(a: TaikoDifficultyAttributes) -> proto::TaikoDifficultyAttributes {
    proto::TaikoDifficultyAttributes {
        stamina: a.stamina,
        rhythm: a.rhythm,
        color: a.color,
        reading: a.reading,
        great_hit_window: a.great_hit_window,
        ok_hit_window: a.ok_hit_window,
        mono_stamina_factor: a.mono_stamina_factor,
        is_convert: a.is_convert,
    }
}

fn catch_diff(a: CatchDifficultyAttributes) -> proto::CatchDifficultyAttributes {
    proto::CatchDifficultyAttributes {
        n_fruits: a.n_fruits,
        n_droplets: a.n_droplets,
        n_tiny_droplets: a.n_tiny_droplets,
        is_convert: a.is_convert,
        ..Default::default()
    }
}

fn mania_diff(a: ManiaDifficultyAttributes) -> proto::ManiaDifficultyAttributes {
    proto::ManiaDifficultyAttributes {
        n_objects: a.n_objects,
        n_hold_notes: a.n_hold_notes,
        is_convert: a.is_convert,
    }
}

fn osu_perf(a: OsuPerformanceAttributes, cs: f64) -> proto::OsuPerformanceAttributes {
    proto::OsuPerformanceAttributes {
        difficulty: Some(osu_diff(a.difficulty, cs)),
        pp_aim: a.pp_aim,
        pp_speed: a.pp_speed,
        pp_acc: a.pp_acc,
        pp_flashlight: a.pp_flashlight,
        effective_miss_count: a.effective_miss_count,
        speed_deviation: a.speed_deviation,
    }
}

fn taiko_perf(a: TaikoPerformanceAttributes) -> proto::TaikoPerformanceAttributes {
    proto::TaikoPerformanceAttributes {
        difficulty: Some(taiko_diff(a.difficulty)),
        pp_acc: a.pp_acc,
        pp_difficulty: a.pp_difficulty,
        estimated_unstable_rate: a.estimated_unstable_rate,
        ..Default::default()
    }
}

fn catch_perf(a: CatchPerformanceAttributes) -> proto::CatchPerformanceAttributes {
    proto::CatchPerformanceAttributes {
        difficulty: Some(catch_diff(a.difficulty)),
    }
}

fn mania_perf(a: ManiaPerformanceAttributes) -> proto::ManiaPerformanceAttributes {
    proto::ManiaPerformanceAttributes {
        difficulty: Some(mania_diff(a.difficulty)),
        pp_difficulty: a.pp_difficulty,
    }
}

pub fn beatmap_to_proto(map: &Beatmap) -> proto::BeatmapInfoResponse {
    proto::BeatmapInfoResponse {
        version: map.version as u32,
        mode: mode_to_proto(map.mode).into(),
        is_convert: map.is_convert,
        hp: f64::from(map.hp),
        cs: f64::from(map.cs),
        od: f64::from(map.od),
        ar: f64::from(map.ar),
        slider_multiplier: map.slider_multiplier,
        slider_tick_rate: map.slider_tick_rate,
        stack_leniency: f64::from(map.stack_leniency),
        bpm: map.bpm(),
        n_circles: map.hit_objects.iter().filter(|o| o.is_circle()).count() as u32,
        n_sliders: map.hit_objects.iter().filter(|o| o.is_slider()).count() as u32,
        n_spinners: map.hit_objects.iter().filter(|o| o.is_spinner()).count() as u32,
        n_holds: map.hit_objects.iter().filter(|o| o.is_hold_note()).count() as u32,
        n_objects: map.hit_objects.len() as u32,
    }
}

pub fn difficulty_attrs_to_gradual_proto(index: u32, attrs: DifficultyAttributes, cs: f64) -> proto::GradualDifficultyResponse {
    proto::GradualDifficultyResponse {
        object_index: index,
        stars: attrs.stars(),
        max_combo: attrs.max_combo(),
        attributes: Some(match attrs {
            DifficultyAttributes::Osu(a) => proto::gradual_difficulty_response::Attributes::Osu(osu_diff(a, cs)),
            DifficultyAttributes::Taiko(a) => proto::gradual_difficulty_response::Attributes::Taiko(taiko_diff(a)),
            DifficultyAttributes::Catch(a) => proto::gradual_difficulty_response::Attributes::Catch(catch_diff(a)),
            DifficultyAttributes::Mania(a) => proto::gradual_difficulty_response::Attributes::Mania(mania_diff(a)),
        }),
    }
}

pub fn apply_score_params_to_state(state: &mut ScoreState, s: &ScoreParams) {
    if let Some(v) = s.combo { state.max_combo = v; }
    if let Some(v) = s.n300 { state.n300 = v; }
    if let Some(v) = s.n100 { state.n100 = v; }
    if let Some(v) = s.n50 { state.n50 = v; }
    if let Some(v) = s.n_geki { state.n_geki = v; }
    if let Some(v) = s.n_katu { state.n_katu = v; }
    if let Some(v) = s.misses { state.misses = v; }
}
