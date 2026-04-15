use std::sync::Arc;

use rayon::prelude::*;
use rosu_mods::GameModsIntermode;
use rosu_pp::any::ScoreState;
use rosu_pp::{Beatmap, Difficulty, GameMods, GradualDifficulty, GradualPerformance, Performance};

use super::convert::{
    apply_difficulty_settings, apply_score_params, apply_score_params_to_state, beatmap_to_proto,
    difficulty_attrs_to_gradual_proto, difficulty_to_proto, effective_cs, parse_beatmap,
    performance_to_proto, proto_to_mode, strains_to_proto,
};
use super::graph;
use crate::error::{Error, Result};
use crate::proto::{
    BatchRequest, BatchResponse, BeatmapInfoRequest, BeatmapInfoResponse, DifficultyRequest,
    DifficultyResponse, DifficultySettings, GradualDifficultyRequest, GradualPerformanceRequest,
    PerformanceRequest, PerformanceResponse, StrainGraphRequest, StrainGraphResponse,
    StrainsRequest, StrainsResponse,
};

const MAX_BATCH_SIZE: usize = 100;

pub struct Calculator;

impl Calculator {
    pub fn difficulty(req: &DifficultyRequest) -> Result<DifficultyResponse> {
        let map = Self::load_and_convert(req.beatmap.as_ref(), req.mode(), req.settings.as_ref())?;
        let cs = effective_cs(map.cs, req.settings.as_ref());
        let difficulty = Self::build_difficulty(req.settings.as_ref());
        Ok(difficulty_to_proto(difficulty.calculate(&map), cs))
    }

    pub fn performance(req: &PerformanceRequest) -> Result<PerformanceResponse> {
        let map = Self::load_and_convert(
            req.beatmap.as_ref(),
            req.mode(),
            req.difficulty_settings.as_ref(),
        )?;
        let cs = effective_cs(map.cs, req.difficulty_settings.as_ref());
        let difficulty = Self::build_difficulty(req.difficulty_settings.as_ref());

        let diff_attrs = difficulty.calculate(&map);
        let mut perf = Performance::new(diff_attrs);

        if let Some(s) = &req.difficulty_settings {
            // Prefer mods_str (acronym-based) over mods (bitflags) for lazer mod support (CL, etc.)
            if let Some(ref mods_str) = s.mods_str {
                match mods_str.parse::<GameModsIntermode>() {
                    Ok(game_mods) => perf = perf.mods(game_mods),
                    Err(_) => if let Some(m) = s.mods { perf = perf.mods(m); }
                }
            } else if let Some(m) = s.mods {
                perf = perf.mods(m);
            }
            if let Some(rate) = s.clock_rate {
                perf = perf.clock_rate(rate);
            }
            if let Some(lazer) = s.lazer {
                perf = perf.lazer(lazer);
            }
        }

        if let Some(score) = &req.score {
            perf = apply_score_params(perf, score);
        }

        Ok(performance_to_proto(perf.calculate(), cs))
    }

    pub fn batch(req: &BatchRequest) -> Result<BatchResponse> {
        if req.requests.len() > MAX_BATCH_SIZE {
            return Err(Error::BatchTooLarge(req.requests.len(), MAX_BATCH_SIZE));
        }

        req.requests
            .par_iter()
            .map(Self::performance)
            .collect::<Result<Vec<_>>>()
            .map(|responses| BatchResponse { responses })
    }

    pub fn strains(req: &StrainsRequest) -> Result<StrainsResponse> {
        let map = Self::load_and_convert(req.beatmap.as_ref(), req.mode(), req.settings.as_ref())?;
        let difficulty = Self::build_difficulty(req.settings.as_ref());
        Ok(strains_to_proto(difficulty.strains(&map)))
    }

    pub fn strain_graph(req: &StrainGraphRequest) -> Result<StrainGraphResponse> {
        let map = Self::load_and_convert(req.beatmap.as_ref(), req.mode(), req.settings.as_ref())?;

        // Parse mods: prefer mods_str (acronym-based) over mods (bitflags)
        let mods: GameMods = if let Some(settings) = &req.settings {
            if let Some(ref mods_str) = settings.mods_str {
                match mods_str.parse::<GameModsIntermode>() {
                    Ok(game_mods) => game_mods.into(),
                    Err(_) => settings.mods.unwrap_or(0).into(),
                }
            } else {
                settings.mods.unwrap_or(0).into()
            }
        } else {
            GameMods::default()
        };

        let background = req.background.as_deref();

        let png_data = graph::render_strain_graph(
            &map,
            mods,
            req.width,
            req.height,
            background,
        )?;

        Ok(StrainGraphResponse { png_data })
    }

    pub fn beatmap_info(req: &BeatmapInfoRequest) -> Result<BeatmapInfoResponse> {
        let map = Self::load_beatmap(req.beatmap.as_ref())?;
        Ok(beatmap_to_proto(&map))
    }

    pub fn gradual_difficulty(req: &GradualDifficultyRequest) -> Result<GradualDifficultyIter> {
        let map = Self::load_and_convert(req.beatmap.as_ref(), req.mode(), req.settings.as_ref())?;
        let cs = effective_cs(map.cs, req.settings.as_ref());
        let difficulty = Self::build_difficulty(req.settings.as_ref());
        Ok(GradualDifficultyIter::new(difficulty.gradual_difficulty(&map), cs))
    }

    pub fn gradual_performance(req: &GradualPerformanceRequest) -> Result<GradualPerformanceCalc> {
        let map = Self::load_and_convert(
            req.beatmap.as_ref(),
            req.mode(),
            req.difficulty_settings.as_ref(),
        )?;
        let difficulty = Self::build_difficulty(req.difficulty_settings.as_ref());
        let gradual = difficulty.gradual_performance(&map);
        Ok(GradualPerformanceCalc::new(gradual, req))
    }

    fn load_beatmap(source: Option<&crate::proto::BeatmapSource>) -> Result<Arc<Beatmap>> {
        let map = parse_beatmap(source.ok_or(Error::MissingBeatmap)?)?;
        map.check_suspicion().map_err(Error::Suspicious)?;
        Ok(map)
    }

    fn load_and_convert(
        source: Option<&crate::proto::BeatmapSource>,
        mode: crate::proto::GameMode,
        settings: Option<&DifficultySettings>,
    ) -> Result<Arc<Beatmap>> {
        let map = Self::load_beatmap(source)?;

        let Some(target) = proto_to_mode(mode) else {
            return Ok(map);
        };

        if map.mode == target {
            return Ok(map);
        }

        // Need to convert - must clone since we modify
        let mods = settings
            .and_then(|s| s.mods)
            .map_or_else(GameMods::default, GameMods::from);

        let mut owned = Arc::unwrap_or_clone(map);
        owned.convert_mut(target, &mods).map_err(|_| Error::Conversion)?;
        Ok(Arc::new(owned))
    }

    fn build_difficulty(settings: Option<&DifficultySettings>) -> Difficulty {
        settings.map_or_else(Difficulty::new, |s| {
            apply_difficulty_settings(Difficulty::new(), s)
        })
    }
}

pub struct GradualDifficultyIter {
    inner: GradualDifficulty,
    index: u32,
    cs: f64,
}

impl GradualDifficultyIter {
    fn new(inner: GradualDifficulty, cs: f64) -> Self {
        Self { inner, index: 0, cs }
    }
}

impl Iterator for GradualDifficultyIter {
    type Item = crate::proto::GradualDifficultyResponse;

    fn next(&mut self) -> Option<Self::Item> {
        let attrs = self.inner.next()?;
        let resp = difficulty_attrs_to_gradual_proto(self.index, attrs, self.cs);
        self.index += 1;
        Some(resp)
    }
}

/// Gradual PP calculator that streams PP after each hit object.
///
/// Assumes perfect play (all max judgements, no combo breaks) for each object.
/// Initial state from request is used as starting point.
/// Note: Uses n300 for osu!/taiko and n_geki for mania as the max judgement.
pub struct GradualPerformanceCalc {
    inner: GradualPerformance,
    state: ScoreState,
    index: u32,
}

impl GradualPerformanceCalc {
    fn new(inner: GradualPerformance, req: &GradualPerformanceRequest) -> Self {
        let mut state = ScoreState::new();
        if let Some(score) = &req.score {
            apply_score_params_to_state(&mut state, score);
        }
        Self { inner, state, index: 0 }
    }

    pub fn next(&mut self) -> Option<crate::proto::GradualPerformanceResponse> {
        self.state.n300 += 1;
        self.state.max_combo += 1;
        let attrs = self.inner.next(self.state.clone())?;
        let resp = crate::proto::GradualPerformanceResponse {
            object_index: self.index,
            pp: attrs.pp(),
            stars: attrs.stars(),
        };
        self.index += 1;
        Some(resp)
    }
}
