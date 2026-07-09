//! Frame diffing (GPU f32 vs Q32 reference) and PNG output.

use std::io::BufWriter;
use std::path::Path;

/// Per-channel diff statistics over one frame, in normalized [0, 1] units.
#[derive(Debug, Clone, Copy)]
pub struct DiffStats {
    /// Mean absolute delta per channel (r, g, b).
    pub mean_abs: [f64; 3],
    /// Max absolute delta per channel (r, g, b).
    pub max_abs: [f64; 3],
    /// Fraction of pixels whose worst channel delta exceeds 8/255.
    pub frac_over_8bit_8: f64,
}

impl DiffStats {
    /// Worst channel mean, in 8-bit units (for the report table).
    pub fn mean_8bit(&self) -> f64 {
        self.mean_abs.iter().fold(0.0f64, |a, &b| a.max(b)) * 255.0
    }

    /// Worst channel max, in 8-bit units.
    pub fn max_8bit(&self) -> f64 {
        self.max_abs.iter().fold(0.0f64, |a, &b| a.max(b)) * 255.0
    }
}

/// Count non-finite lanes in a GPU frame (Metal fast-math can NaN where the
/// Q32 path saturates — see the m3 report's `tanh` finding).
pub fn count_non_finite(pixels: &[f32]) -> usize {
    pixels.iter().filter(|v| !v.is_finite()).count()
}

/// Quantize a GPU rgba f32 frame with the CPU path's packing rule
/// (Q16.16 raw fraction, truncating, saturating to 65535: `v * 65536`
/// clamped — 1.0 maps to 65535 exactly as on the wasm path). Non-finite
/// lanes quantize to 0 (count them separately via [`count_non_finite`]).
pub fn quantize_gpu_frame(pixels: &[f32]) -> Vec<u16> {
    pixels
        .iter()
        .map(|&v| {
            let raw = (f64::from(v) * 65536.0).floor();
            raw.clamp(0.0, 65535.0) as u16
        })
        .collect()
}

/// Per-pixel stats between two rgba unorm16 frames (alpha ignored: the CPU
/// path saturates authored alpha the same way, and preview cards are opaque).
pub fn diff_frames(reference: &[u16], gpu: &[u16]) -> DiffStats {
    assert_eq!(reference.len(), gpu.len(), "frame size mismatch");
    let pixel_count = reference.len() / 4;
    let mut sum = [0.0f64; 3];
    let mut max = [0.0f64; 3];
    let mut over = 0usize;
    for px in 0..pixel_count {
        let mut worst = 0.0f64;
        for c in 0..3 {
            let a = f64::from(reference[px * 4 + c]) / 65535.0;
            let b = f64::from(gpu[px * 4 + c]) / 65535.0;
            let d = (a - b).abs();
            sum[c] += d;
            max[c] = max[c].max(d);
            worst = worst.max(d);
        }
        if worst > 8.0 / 255.0 {
            over += 1;
        }
    }
    DiffStats {
        mean_abs: sum.map(|s| s / pixel_count as f64),
        max_abs: max,
        frac_over_8bit_8: over as f64 / pixel_count as f64,
    }
}

/// Write a side-by-side grid PNG: one row per timestamp, columns are
/// [reference | gpu | abs-diff ×8], separated by 2px gutters.
pub fn write_side_by_side_grid(
    path: &Path,
    width: u32,
    height: u32,
    frames: &[(Vec<u16>, Vec<u16>)],
) -> std::io::Result<()> {
    const GUTTER: u32 = 2;
    let cols = 3;
    let grid_w = cols * width + (cols - 1) * GUTTER;
    let grid_h = frames.len() as u32 * height + (frames.len() as u32 - 1) * GUTTER;
    let mut rgb = vec![32u8; (grid_w * grid_h * 3) as usize];

    for (row, (reference, gpu)) in frames.iter().enumerate() {
        let oy = row as u32 * (height + GUTTER);
        for y in 0..height {
            for x in 0..width {
                let px = ((y * width + x) * 4) as usize;
                let r16 = [reference[px], reference[px + 1], reference[px + 2]];
                let g16 = [gpu[px], gpu[px + 1], gpu[px + 2]];
                let d16 = [
                    amplified_diff(r16[0], g16[0]),
                    amplified_diff(r16[1], g16[1]),
                    amplified_diff(r16[2], g16[2]),
                ];
                for (col, v) in [r16, g16, d16].iter().enumerate() {
                    let ox = col as u32 * (width + GUTTER);
                    let out = (((oy + y) * grid_w + ox + x) * 3) as usize;
                    rgb[out] = (v[0] >> 8) as u8;
                    rgb[out + 1] = (v[1] >> 8) as u8;
                    rgb[out + 2] = (v[2] >> 8) as u8;
                }
            }
        }
    }
    write_rgb8_png(path, grid_w, grid_h, &rgb)
}

/// Write one rgba unorm16 frame as an 8-bit RGB PNG.
pub fn write_frame_png(path: &Path, width: u32, height: u32, frame: &[u16]) -> std::io::Result<()> {
    let mut rgb = Vec::with_capacity((width * height * 3) as usize);
    for px in frame.chunks_exact(4) {
        rgb.extend_from_slice(&[(px[0] >> 8) as u8, (px[1] >> 8) as u8, (px[2] >> 8) as u8]);
    }
    write_rgb8_png(path, width, height, &rgb)
}

fn amplified_diff(a: u16, b: u16) -> u16 {
    let d = u32::from(a.abs_diff(b)) * 8;
    d.min(65535) as u16
}

fn write_rgb8_png(path: &Path, width: u32, height: u32, rgb: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(std::io::Error::other)?;
    writer
        .write_image_data(rgb)
        .map_err(std::io::Error::other)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantize_matches_wasm_packing_rule() {
        // Same rule the synthesised render loop applies: raw Q16.16 fraction,
        // saturated to 65535 (0.25 → 16384, 1.0 → 65535, negatives → 0).
        let q = quantize_gpu_frame(&[0.25, 1.0, -0.5, 0.5]);
        assert_eq!(q, vec![16384, 65535, 0, 32768]);
    }

    #[test]
    fn diff_stats_on_identical_frames_are_zero() {
        let frame = vec![1000u16, 2000, 3000, 65535, 0, 0, 0, 65535];
        let stats = diff_frames(&frame, &frame);
        assert_eq!(stats.mean_abs, [0.0; 3]);
        assert_eq!(stats.max_abs, [0.0; 3]);
        assert_eq!(stats.frac_over_8bit_8, 0.0);
    }

    #[test]
    fn diff_stats_report_per_channel_delta() {
        let a = vec![0u16, 0, 0, 65535];
        let b = vec![65535u16, 0, 0, 65535];
        let stats = diff_frames(&a, &b);
        assert!((stats.max_abs[0] - 1.0).abs() < 1e-9);
        assert_eq!(stats.max_abs[1], 0.0);
        assert_eq!(stats.frac_over_8bit_8, 1.0);
    }
}
