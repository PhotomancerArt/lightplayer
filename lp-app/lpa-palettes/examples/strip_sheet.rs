//! Renders every catalog palette to one PNG contact sheet — the M3 curation
//! gate artifact ("a rendered strip sheet: one PNG, all palettes").
//!
//! `cargo run -p lpa-palettes --example strip_sheet [output.png]`
//! (defaults to `target/palette-strip-sheet.png`).
//!
//! One row per palette, sampled at `SAMPLES` evenly spaced positions
//! (`InterpMethod::Step` holds the nearest stop at or before `t`;
//! `Linear`/`Smooth` interpolate — `Smooth` renders as linear here, since a
//! contact sheet only needs the color sequence to be legible, not the
//! easing curve). Interpolation happens in the gradient's own colorspace,
//! then the sampled color is converted to display sRGB for the PNG.

use lpa_palettes::{PaletteCategory, all_palettes};
use lpc_model::{Colorspace, Gradient, InterpMethod};

const SAMPLES: usize = 220;
const ROW_HEIGHT: usize = 28;
const LABEL_WIDTH: usize = 260;
const MARGIN: usize = 8;

fn main() {
    let output_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/palette-strip-sheet.png".to_string());

    let palettes = all_palettes();
    let width = LABEL_WIDTH + SAMPLES + 2 * MARGIN;
    let height = palettes.len() * ROW_HEIGHT + 2 * MARGIN;

    let mut pixels = vec![255u8; width * height * 3];

    for (row_index, palette) in palettes.iter().enumerate() {
        let y0 = MARGIN + row_index * ROW_HEIGHT;
        let label = format!("{} [{}]", palette.name, category_tag(palette.category));
        draw_label(&mut pixels, width, MARGIN, y0 + ROW_HEIGHT / 2 - 3, &label);

        for sample in 0..SAMPLES {
            let t = sample as f32 / (SAMPLES - 1) as f32;
            let srgb = sample_gradient_as_srgb(&palette.gradient, t);
            let x0 = MARGIN + LABEL_WIDTH + sample;
            for y in y0..(y0 + ROW_HEIGHT - 2) {
                let offset = (y * width + x0) * 3;
                pixels[offset] = (srgb[0] * 255.0).round().clamp(0.0, 255.0) as u8;
                pixels[offset + 1] = (srgb[1] * 255.0).round().clamp(0.0, 255.0) as u8;
                pixels[offset + 2] = (srgb[2] * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }

    write_png(&output_path, width, height, &pixels);

    // A row-ordered legend: this crate deliberately skips embedding a
    // bitmap font (not worth the code for a dev-only curation artifact),
    // so the PNG's per-row label area is a density tick mark, not legible
    // text. This legend is the authoritative row -> palette mapping.
    let legend_path = format!("{output_path}.legend.txt");
    let legend = palettes
        .iter()
        .enumerate()
        .map(|(index, palette)| {
            let license = palette
                .license
                .as_ref()
                .map(|l| l.spdx.as_str())
                .unwrap_or("original");
            format!(
                "row {index:02}: {} [{}] {license}",
                palette.name,
                category_tag(palette.category)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&legend_path, legend + "\n")
        .unwrap_or_else(|e| panic!("write {legend_path}: {e}"));

    println!(
        "wrote {} palettes to {output_path} ({width}x{height}), legend at {legend_path}",
        palettes.len()
    );
}

fn category_tag(category: PaletteCategory) -> &'static str {
    match category {
        PaletteCategory::FastledStock => "fastled",
        PaletteCategory::CptCity => "cpt-city",
        PaletteCategory::LightplayerOriginal => "original",
    }
}

/// Sample `gradient` at `t` and convert the result to display sRGB.
fn sample_gradient_as_srgb(gradient: &Gradient, t: f32) -> [f32; 3] {
    let mut stops = gradient.stops.clone();
    stops.sort_by(|a, b| a.at.total_cmp(&b.at));

    let raw = match gradient.method {
        InterpMethod::Step => sample_step(&stops, t),
        InterpMethod::Linear | InterpMethod::Smooth => sample_linear(&stops, t),
    };

    to_display_srgb(gradient.space, raw)
}

fn sample_step(stops: &[lpc_model::GradientStop], t: f32) -> [f32; 3] {
    let mut chosen = stops[0].c;
    for stop in stops {
        if stop.at <= t {
            chosen = stop.c;
        } else {
            break;
        }
    }
    chosen
}

fn sample_linear(stops: &[lpc_model::GradientStop], t: f32) -> [f32; 3] {
    if t <= stops[0].at {
        return stops[0].c;
    }
    let last = stops.len() - 1;
    if t >= stops[last].at {
        return stops[last].c;
    }
    for window in stops.windows(2) {
        let [a, b] = window else { unreachable!() };
        if t >= a.at && t <= b.at {
            let span = (b.at - a.at).max(f32::EPSILON);
            let f = (t - a.at) / span;
            return [
                a.c[0] + (b.c[0] - a.c[0]) * f,
                a.c[1] + (b.c[1] - a.c[1]) * f,
                a.c[2] + (b.c[2] - a.c[2]) * f,
            ];
        }
    }
    stops[last].c
}

/// Convert a sampled color from the gradient's authoring space to display
/// (gamma-encoded) sRGB, for the PNG. `Srgb` is already display-encoded;
/// `Oklab` goes through the standard Oklab -> linear sRGB matrices, then
/// gamma-encodes.
fn to_display_srgb(space: Colorspace, c: [f32; 3]) -> [f32; 3] {
    match space {
        Colorspace::Srgb => c,
        Colorspace::LinearSrgb => c.map(linear_to_srgb),
        Colorspace::Oklab => oklab_to_display_srgb(c),
        // Hsl/Hsv/Oklch aren't used by the M3 catalog; fall back to a
        // clamp so the sheet never panics if one is added later.
        Colorspace::Hsl | Colorspace::Hsv | Colorspace::Oklch => [
            c[0].clamp(0.0, 1.0),
            c[1].clamp(0.0, 1.0),
            c[2].clamp(0.0, 1.0),
        ],
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        (c * 12.92).clamp(0.0, 1.0)
    } else {
        (1.055 * c.powf(1.0 / 2.4) - 0.055).clamp(0.0, 1.0)
    }
}

fn oklab_to_display_srgb(lab: [f32; 3]) -> [f32; 3] {
    let [l, a, b] = lab;
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;

    let l3 = l_ * l_ * l_;
    let m3 = m_ * m_ * m_;
    let s3 = s_ * s_ * s_;

    let r = 4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3;
    let g = -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_38 * s3;
    let bl = -0.004_196_086_3 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3;

    [linear_to_srgb(r), linear_to_srgb(g), linear_to_srgb(bl)]
}

/// One tick mark per non-space character — not legible text (a bitmap font
/// is a lot of code for a dev-only artifact). Row identity comes from the
/// `.legend.txt` file written alongside the PNG, not this image.
fn draw_label(pixels: &mut [u8], width: usize, x0: usize, y: usize, label: &str) {
    for (index, ch) in label.chars().enumerate() {
        if ch == ' ' {
            continue;
        }
        let x = x0 + (index * 4).min(LABEL_WIDTH.saturating_sub(4));
        let offset = (y * width + x) * 3;
        if offset + 2 < pixels.len() {
            pixels[offset] = 20;
            pixels[offset + 1] = 20;
            pixels[offset + 2] = 20;
        }
    }
}

fn write_png(path: &str, width: usize, height: usize, rgb: &[u8]) {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let file = std::fs::File::create(path).unwrap_or_else(|e| panic!("create {path}: {e}"));
    let writer = std::io::BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .unwrap_or_else(|e| panic!("write PNG header: {e}"));
    writer
        .write_image_data(rgb)
        .unwrap_or_else(|e| panic!("write PNG data: {e}"));
}
