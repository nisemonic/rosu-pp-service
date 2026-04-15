//! Strain graph rendering module.
//!
//! Generates PNG images showing beatmap strain distribution over time.

use std::{cell::RefCell, mem, rc::Rc, time::Duration};

use enterpolation::{linear::Linear, Curve};
use plotters::{
    coord::{types::RangedCoordf64, Shift},
    prelude::*,
};
use plotters_skia::SkiaBackend;
use rosu_pp::{
    any::Strains,
    catch::CatchStrains,
    mania::ManiaStrains,
    osu::OsuStrains,
    taiko::TaikoStrains,
    Beatmap, Difficulty, GameMods,
};
use skia_safe::{BlendMode, EncodedImageFormat, surfaces};

use crate::error::{Error, Result};

/// Number of strain points after interpolation (for consistent graph smoothness)
const NEW_STRAIN_COUNT: usize = 200;

/// Legend height in pixels
const LEGEND_H: u32 = 25;

/// Default graph width
pub const DEFAULT_WIDTH: u32 = 900;

/// Default graph height
pub const DEFAULT_HEIGHT: u32 = 250;

/// Generate a strain graph PNG for the given beatmap.
///
/// # Arguments
/// * `map` - Parsed beatmap
/// * `mods` - Mods to apply (supports both bitflags and lazer mods)
/// * `width` - Graph width in pixels
/// * `height` - Graph height in pixels
/// * `background` - Optional background image bytes (PNG/JPEG)
///
/// # Returns
/// PNG image bytes
pub fn render_strain_graph(
    map: &Beatmap,
    mods: impl Into<GameMods>,
    width: u32,
    height: u32,
    background: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let mods = mods.into();
    let w = if width == 0 { DEFAULT_WIDTH } else { width };
    let h = if height == 0 { DEFAULT_HEIGHT } else { height };

    let strains = GraphStrains::new(map, mods)?;

    let last_timestamp = ((NEW_STRAIN_COUNT - 2) as f64
        * strains.strains.section_len()
        * strains.strains_count as f64)
        / NEW_STRAIN_COUNT as f64;

    let max_strain = calculate_max_strain(&strains.strains);

    if max_strain <= f64::EPSILON {
        return Err(Error::NoStrainData);
    }

    let mut surface = surfaces::raster_n32_premul((w as i32, h as i32))
        .ok_or(Error::GraphRendering("Failed to create surface".into()))?;

    {
        let backend = Rc::new(RefCell::new(SkiaBackend::new(surface.canvas(), w, h)));
        let root = DrawingArea::from(&backend);

        // Draw background
        draw_background(&root, w, h, background)?;

        let (legend_area, graph_area) = root.split_vertically(LEGEND_H);

        let mut chart = ChartBuilder::on(&graph_area)
            .x_label_area_size(17_i32)
            .build_cartesian_2d(last_timestamp.min(1.0)..last_timestamp, 0.0_f64..max_strain)
            .map_err(|e| Error::GraphRendering(e.to_string()))?;

        // Configure mesh and labels
        let text_style = FontDesc::new(FontFamily::SansSerif, 14.0, FontStyle::Bold).color(&WHITE);

        chart
            .configure_mesh()
            .disable_y_mesh()
            .disable_y_axis()
            .set_all_tick_mark_size(3_i32)
            .light_line_style(WHITE.mix(0.0))
            .bold_line_style(WHITE.mix(0.75))
            .x_labels(10)
            .x_label_style(text_style.clone())
            .axis_style(WHITE)
            .x_label_formatter(&|timestamp| {
                if timestamp.abs() < f64::EPSILON {
                    return String::new();
                }
                let d = Duration::from_millis(*timestamp as u64);
                let minutes = d.as_secs() / 60;
                let seconds = d.as_secs() % 60;
                format!("{minutes}:{seconds:0>2}")
            })
            .draw()
            .map_err(|e| Error::GraphRendering(e.to_string()))?;

        draw_mode_strains(&backend, &mut chart, strains, &legend_area, &text_style)?;
    }

    let png_bytes = surface
        .image_snapshot()
        .encode(None, EncodedImageFormat::PNG, None)
        .ok_or(Error::GraphRendering("Failed to encode PNG".into()))?
        .to_vec();

    Ok(png_bytes)
}

fn calculate_max_strain(strains: &Strains) -> f64 {
    match strains {
        Strains::Osu(OsuStrains {
            aim,
            aim_no_sliders,
            speed,
            flashlight,
        }) => aim
            .iter()
            .zip(aim_no_sliders)
            .zip(speed)
            .zip(flashlight)
            .fold(0.0_f64, |max, (((a, b), c), d)| {
                max.max(*a).max(*b).max(*c).max(*d)
            }),
        Strains::Taiko(TaikoStrains {
            color,
            reading,
            rhythm,
            stamina,
            single_color_stamina,
        }) => color
            .iter()
            .zip(rhythm)
            .zip(stamina)
            .zip(single_color_stamina)
            .zip(reading)
            .fold(0.0_f64, |max, ((((a, b), c), d), e)| {
                max.max(*a).max(*b).max(*c).max(*d).max(*e)
            }),
        Strains::Catch(CatchStrains { movement }) => movement
            .iter()
            .fold(0.0_f64, |max, strain| max.max(*strain)),
        Strains::Mania(ManiaStrains { strains }) => strains
            .iter()
            .fold(0.0_f64, |max, strain| max.max(*strain)),
    }
}

fn draw_background(
    root: &DrawingArea<SkiaBackend<'_>, Shift>,
    w: u32,
    h: u32,
    background: Option<&[u8]>,
) -> Result<()> {
    if let Some(bg_bytes) = background {
        // Try to load and draw background image
        if let Ok(img) = image::load_from_memory(bg_bytes) {
            let resized = img.resize_exact(w, h, image::imageops::FilterType::Triangle);
            let _rgba = resized.to_rgba8();

            // TODO: Draw background image using skia blit
            // For now, just fill with dark color and overlay
            root.fill(&RGBColor(19, 43, 33))
                .map_err(|e| Error::GraphRendering(e.to_string()))?;

            // Draw darkening overlay
            let rect = Rectangle::new([(0, 0), (w as i32, h as i32)], BLACK.mix(0.75).filled());
            root.draw(&rect)
                .map_err(|e| Error::GraphRendering(e.to_string()))?;

            return Ok(());
        }
    }

    // Default dark background
    root.fill(&RGBColor(19, 43, 33))
        .map_err(|e| Error::GraphRendering(e.to_string()))?;

    Ok(())
}

fn draw_mode_strains(
    backend: &Rc<RefCell<SkiaBackend<'_>>>,
    chart: &mut ChartContext<'_, SkiaBackend<'_>, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
    strains: GraphStrains,
    legend_area: &DrawingArea<SkiaBackend<'_>, Shift>,
    text_style: &TextStyle<'_>,
) -> Result<()> {
    let GraphStrains {
        strains,
        strains_count,
    } = strains;

    let orig_count = strains_count as f64;

    let new_count = match strains {
        Strains::Osu(ref s) => s.aim.len(),
        Strains::Taiko(ref s) => s.color.len(),
        Strains::Catch(ref s) => s.movement.len(),
        Strains::Mania(ref s) => s.strains.len(),
    } as f64;

    let section_len = strains.section_len();
    let mut legend_x: i32 = 8;
    let factor = section_len * orig_count / new_count;

    macro_rules! draw_line {
        ($label:literal, $strains:expr, $color:ident) => {{
            draw_series(backend, chart, &$strains, $label, factor, $color)?;
            draw_legend_item(legend_area, $label, $color, text_style, &mut legend_x)?;
        }};
    }

    match strains {
        Strains::Osu(s) => {
            draw_line!("Aim", s.aim, CYAN);
            draw_line!("Aim (Sliders)", s.aim_no_sliders, GREEN);
            draw_line!("Speed", s.speed, RED);
            draw_line!("Flashlight", s.flashlight, MAGENTA);
        }
        Strains::Taiko(s) => {
            draw_line!("Stamina", s.stamina, RED);
            draw_line!("Stamina (SC)", s.single_color_stamina, BLUE);
            draw_line!("Color", s.color, YELLOW);
            draw_line!("Rhythm", s.rhythm, CYAN);
            draw_line!("Reading", s.reading, GREEN);
        }
        Strains::Catch(s) => {
            draw_line!("Movement", s.movement, CYAN);
        }
        Strains::Mania(s) => {
            draw_line!("Strain", s.strains, MAGENTA);
        }
    }

    Ok(())
}

fn draw_series(
    backend: &Rc<RefCell<SkiaBackend<'_>>>,
    chart: &mut ChartContext<'_, SkiaBackend<'_>, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
    strains: &[f64],
    label: &str,
    factor: f64,
    color: RGBColor,
) -> Result<()> {
    backend.borrow_mut().set_blend_mode(Some(BlendMode::Lighten));

    let timestamp_iter = strains
        .iter()
        .enumerate()
        .map(move |(i, strain)| (i as f64 * factor, *strain));

    let series = AreaSeries::new(timestamp_iter, 0.0, color.mix(0.20))
        .border_style(color.stroke_width(2));

    chart
        .draw_series(series)
        .map_err(|e| Error::GraphRendering(format!("Failed to draw {label} series: {e}")))?;

    backend.borrow_mut().set_blend_mode(None);

    Ok(())
}

fn draw_legend_item(
    legend_area: &DrawingArea<SkiaBackend<'_>, Shift>,
    label: &str,
    color: RGBColor,
    text_style: &TextStyle<'_>,
    legend_x: &mut i32,
) -> Result<()> {
    // Draw colored rectangle
    let rect = Rectangle::new(
        [
            (*legend_x, (LEGEND_H as f32 * 0.42) as i32),
            (*legend_x + 16, (LEGEND_H as f32 * 0.58) as i32),
        ],
        color.filled(),
    );

    legend_area
        .draw(&rect)
        .map_err(|e| Error::GraphRendering(format!("Failed to draw legend rect: {e}")))?;

    *legend_x += 26;

    // Get text dimensions
    let ((min_x, _), (max_x, max_y)) = text_style
        .font
        .layout_box(label)
        .map_err(|e| Error::GraphRendering(format!("Failed to get layout box: {e}")))?;

    let width = max_x - min_x;

    let text_pos = (*legend_x, (LEGEND_H as i32 - 8 - max_y));

    legend_area
        .draw_text(label, text_style, text_pos)
        .map_err(|e| Error::GraphRendering(format!("Failed to draw legend text: {e}")))?;

    *legend_x += width + 10;

    Ok(())
}

/// Smoothed strain data ready for graphing
struct GraphStrains {
    /// Interpolated strain values (200 points)
    strains: Strains,
    /// Original number of strain sections
    strains_count: usize,
}

impl GraphStrains {
    fn new(map: &Beatmap, mods: GameMods) -> Result<Self> {
        let mut strains = Difficulty::new().mods(mods).strains(map);
        let section_len = strains.section_len();

        let strains_count = match strains {
            Strains::Osu(ref s) => s.aim.len(),
            Strains::Taiko(ref s) => s.color.len(),
            Strains::Catch(ref s) => s.movement.len(),
            Strains::Mania(ref s) => s.strains.len(),
        };

        if strains_count == 0 {
            return Err(Error::NoStrainData);
        }

        let create_curve = |strains: Vec<f64>| -> Result<Vec<f64>> {
            if strains.is_empty() {
                return Ok(vec![0.0; NEW_STRAIN_COUNT]);
            }
            if strains.len() == 1 {
                return Ok(vec![strains[0]; NEW_STRAIN_COUNT]);
            }

            Linear::builder()
                .elements(strains)
                .equidistant()
                .distance(0.0, section_len)
                .build()
                .map(|curve| curve.take(NEW_STRAIN_COUNT).collect())
                .map_err(|e| Error::GraphRendering(format!("Failed to build curve: {e}")))
        };

        match &mut strains {
            Strains::Osu(OsuStrains {
                aim,
                aim_no_sliders,
                speed,
                flashlight,
            }) => {
                // Calculate slider aim contribution
                aim.iter()
                    .zip(aim_no_sliders.iter_mut())
                    .for_each(|(aim_val, no_slider)| *no_slider = *aim_val - *no_slider);

                *aim = create_curve(mem::take(aim))?;
                *aim_no_sliders = create_curve(mem::take(aim_no_sliders))?;
                *speed = create_curve(mem::take(speed))?;
                *flashlight = create_curve(mem::take(flashlight))?;
            }
            Strains::Taiko(TaikoStrains {
                color,
                reading,
                rhythm,
                stamina,
                single_color_stamina,
            }) => {
                *color = create_curve(mem::take(color))?;
                *rhythm = create_curve(mem::take(rhythm))?;
                *stamina = create_curve(mem::take(stamina))?;
                *single_color_stamina = create_curve(mem::take(single_color_stamina))?;
                *reading = create_curve(mem::take(reading))?;
            }
            Strains::Catch(CatchStrains { movement }) => {
                *movement = create_curve(mem::take(movement))?;
            }
            Strains::Mania(ManiaStrains { strains }) => {
                *strains = create_curve(mem::take(strains))?;
            }
        }

        Ok(Self {
            strains,
            strains_count,
        })
    }
}
