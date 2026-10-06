//! An art piece as a picture: one frame of its LightPlayer project, rendered
//! on the host engine (the same path as `lp-cli pattern preview`), drawn as
//! glowing LEDs at the lamp positions of the project's own 2D mapping.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lpfs::LpFsStd;

use crate::commands::pattern::preview_render;
use crate::commands::pattern::swatch::Swatch;

/// The drawing's long side, in SVG units.
const LONG_SIDE: f32 = 1000.0;
/// Frame rate the project is ticked at on its way to `at_seconds`.
const FPS: u32 = 30;
/// A dim piece is brightened for the picture, up to this factor: the choker
/// runs at low brightness on purpose, and a photo of it would not look black.
const MAX_BOOST: f32 = 6.0;

/// The frame `at_seconds` into `project_dir`, drawn.
pub fn art_svg(project_dir: &Path, at_seconds: f32) -> Result<String> {
    let map_path = mapping_file(project_dir)?;
    let swatch = Swatch::read(&map_path)?;
    let fs = LpFsStd::new(project_dir.to_path_buf());
    let frames = preview_render::record(&fs, "desk_art", swatch.lamp_count(), at_seconds, FPS)
        .with_context(|| format!("render {}", project_dir.display()))?;
    let stride = frames.lamp_count * 3;
    let last = &frames.rgb[frames.rgb.len() - stride..];
    let colours: Vec<[u8; 3]> = last
        .chunks_exact(3)
        .map(|rgb| [rgb[0], rgb[1], rgb[2]])
        .collect();
    let (positions, size) = swatch.drawing_positions();
    Ok(draw(
        &positions,
        size,
        Swatch::drawing_pitch(&positions),
        &colours,
    ))
}

/// The project's 2D mapping: its one `*.map2d.json`.
fn mapping_file(project_dir: &Path) -> Result<PathBuf> {
    let mut maps: Vec<PathBuf> = std::fs::read_dir(project_dir)
        .with_context(|| format!("read {}", project_dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.to_string_lossy().ends_with(".map2d.json"))
        .collect();
    maps.sort();
    match maps.as_slice() {
        [one] => Ok(one.clone()),
        [] => bail!(
            "{} has no *.map2d.json to draw the piece on",
            project_dir.display()
        ),
        _ => bail!(
            "{} has several *.map2d.json files; which one is the piece is not decided here",
            project_dir.display()
        ),
    }
}

/// Lamps at `positions` (long side 0 → 1, y down), `pitch` apart, lit
/// `colours`, on a dark card. Each lamp is a soft halo under a bright core;
/// an unlit lamp still shows as a faint ring, so the piece's shape reads.
fn draw(positions: &[[f32; 2]], size: [f32; 2], pitch: f32, colours: &[[u8; 3]]) -> String {
    let margin = pitch * 1.5;
    let scale = LONG_SIDE / (size[0].max(size[1]) + 2.0 * margin);
    let width = (size[0] + 2.0 * margin) * scale;
    let height = (size[1] + 2.0 * margin) * scale;
    let peak = colours.iter().flatten().copied().max().unwrap_or(0) as f32;
    let boost = if peak > 0.0 {
        (255.0 / peak).min(MAX_BOOST)
    } else {
        1.0
    };
    let core = pitch * scale * 0.22;
    let halo = pitch * scale * 0.55;

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width:.1} {height:.1}\">\
         <defs><filter id=\"glow\" x=\"-50%\" y=\"-50%\" width=\"200%\" height=\"200%\">\
         <feGaussianBlur stdDeviation=\"{:.2}\"/></filter></defs>\
         <rect width=\"{width:.1}\" height=\"{height:.1}\" rx=\"{:.1}\" fill=\"#08080c\"/>",
        halo * 0.5,
        LONG_SIDE * 0.012,
    );
    let at = |[x, y]: [f32; 2]| ((x + margin) * scale, (y + margin) * scale);
    let lit = |colour: &[u8; 3]| -> String {
        let [r, g, b] = colour.map(|channel| (channel as f32 * boost).round().min(255.0) as u8);
        format!("#{r:02x}{g:02x}{b:02x}")
    };
    svg.push_str("<g filter=\"url(#glow)\" opacity=\"0.85\">");
    for (position, colour) in positions.iter().zip(colours) {
        if colour.iter().all(|channel| *channel == 0) {
            continue;
        }
        let (x, y) = at(*position);
        svg.push_str(&format!(
            "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"{halo:.1}\" fill=\"{}\"/>",
            lit(colour)
        ));
    }
    svg.push_str("</g><g>");
    for (position, colour) in positions.iter().zip(colours) {
        let (x, y) = at(*position);
        svg.push_str(&format!(
            "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"{core:.1}\" fill=\"{}\" stroke=\"#2a2a33\" stroke-width=\"{:.1}\"/>",
            lit(colour),
            core * 0.25
        ));
    }
    svg.push_str("</g></svg>\n");
    svg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lamps_land_where_the_mapping_puts_them_with_a_halo_only_when_lit() {
        let svg = draw(
            &[[0.0, 0.0], [1.0, 0.0]],
            [1.0, 0.0],
            1.0,
            &[[255, 0, 0], [0, 0, 0]],
        );
        // 1.5 pitches of margin each side: 4 units across, 3 down, 1000 long.
        assert!(svg.contains("viewBox=\"0 0 1000.0 750.0\""), "{svg}");
        assert!(svg.contains("<circle cx=\"375.0\" cy=\"375.0\" r=\"137.5\" fill=\"#ff0000\"/>"));
        assert_eq!(
            svg.matches("r=\"137.5\"").count(),
            1,
            "the unlit lamp has no halo"
        );
        assert!(
            svg.contains("cx=\"625.0\" cy=\"375.0\" r=\"55.0\" fill=\"#000000\""),
            "but keeps its ring"
        );
    }

    #[test]
    fn a_dim_piece_is_brightened_for_the_picture_but_only_so_far() {
        let dim = draw(&[[0.0, 0.0]], [0.0, 0.0], 1.0, &[[0, 51, 0]]);
        assert!(dim.contains("fill=\"#00ff00\""), "51 × 5 = 255");
        let very_dim = draw(&[[0.0, 0.0]], [0.0, 0.0], 1.0, &[[0, 10, 0]]);
        assert!(very_dim.contains("fill=\"#003c00\""), "capped at ×6 = 60");
    }

    #[test]
    fn the_playful_choker_renders() {
        let project =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../catalog/projects/playful-choker");
        let svg = art_svg(&project, 2.0).unwrap();
        assert!(svg.matches("r=\"").count() >= 73, "every lamp is drawn");
        assert!(svg.contains("filter=\"url(#glow)\""));
    }
}
