//! Fixture-preview contracts for control products: the per-tick resolution
//! result presenters splat with, and the CPU-tier rasterizer that draws the
//! same splat host-side from Display-policy probe bytes.
//!
//! Both tiers show the same color (D1/D2): raw sampled color × brightness,
//! no gamma, no color-order permutation. On the GPU tier the brightness
//! rides [`lp_gfx::LedSplatParams::color_scale`]
//! ([`ControlPreviewSpec::color_scale`]); on the CPU tier the probe's
//! Display bytes already carry it, so [`rasterize_control_preview_rgba16`]
//! splats them unscaled.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lp_gfx::{LedSplatInstance, LedSplatRasterParams, rasterize_led_splats};
use lpc_model::ControlLayout2d;

/// What a presenter needs to splat a fixture's GPU-resident sample grid.
///
/// Returned by `Engine::resolve_fixture_preview`. Grid texel `i` (row-major,
/// see [`lp_gfx::LpShader::sample_to_grid`]) holds the sampled color of
/// `layout.lamps[i]` — instances built from lamp `i` must fetch
/// `grid_index = i`, not the lamp's `lamp_index` channel.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlPreviewSpec {
    /// Number of sample points in the resident grid (== `layout.lamps.len()`).
    /// Presenters size the grid target with
    /// [`lp_gfx::LpGraphics::sample_grid_dims`] from this count.
    pub point_count: u32,
    /// Display layout: normalized `[0, 1]` lamp centers and radii, y down.
    /// `revision` changes when the layout does — the caller's cue to
    /// recreate the grid target and instance list.
    pub layout: ControlLayout2d,
    /// Display-policy color factor (D1): the producer's brightness as
    /// `brightness / 255`. Pass as [`lp_gfx::LedSplatParams::color_scale`]
    /// so the splat shows raw sampled color × brightness.
    pub color_scale: f32,
}

/// Splat styling shared by CPU-tier callers (defaults live with the caller,
/// mirroring the GPU op).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlPreviewRasterStyle {
    /// Global multiplier on every lamp radius.
    pub radius_scale: f32,
    /// Background the splat accumulates over (linear unorm-range RGBA).
    pub clear_color: [f32; 4],
}

/// CPU-tier fixture preview: rasterize **Display-policy** control probe
/// bytes (`WireChannelSampleFormat::U16`, little-endian) into `width ×
/// height` RGBA16 channels using the same visual model as the GPU splat op.
///
/// Display bytes are RGB-ordered with brightness already applied, so lamp
/// `i` splats color `bytes[lamp.sample_start .. +3]` directly
/// (`color_scale = 1`). Feeding Wire-policy bytes here would show
/// gamma/color-order corrections that D1 excludes — request
/// `ControlColorPolicy::Display` on the probe.
pub fn rasterize_control_preview_rgba16(
    bytes: &[u8],
    layout: &ControlLayout2d,
    style: &ControlPreviewRasterStyle,
    width: u32,
    height: u32,
) -> Result<Vec<u16>, String> {
    if bytes.len() % 2 != 0 {
        return Err(String::from(
            "control preview bytes are not u16 little-endian samples",
        ));
    }
    let samples: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();

    let mut grid_channels = Vec::with_capacity(layout.lamps.len() * 4);
    let mut instances = Vec::with_capacity(layout.lamps.len());
    for (index, lamp) in layout.lamps.iter().enumerate() {
        let start = lamp.sample_start as usize;
        let rgb = samples.get(start..start + 3).ok_or_else(|| {
            format!(
                "lamp {} sample_start {} exceeds the {} control samples provided",
                lamp.lamp_index,
                lamp.sample_start,
                samples.len()
            )
        })?;
        grid_channels.extend_from_slice(&[rgb[0], rgb[1], rgb[2], u16::MAX]);
        instances.push(LedSplatInstance {
            position: [lamp.center[0], lamp.center[1], 0.0],
            radius: lamp.radius,
            grid_index: index as u32,
        });
    }

    rasterize_led_splats(
        &grid_channels,
        &instances,
        &LedSplatRasterParams {
            world_min: [0.0, 0.0],
            world_max: [1.0, 1.0],
            radius_scale: style.radius_scale,
            // Display bytes already carry brightness; never scale twice.
            color_scale: 1.0,
            clear_color: style.clear_color,
        },
        width,
        height,
    )
    .map_err(|error| format!("control preview raster: {error}"))
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use lpc_model::{ControlLamp2d, Revision};

    #[test]
    fn display_bytes_splat_at_lamp_centers_by_sample_start() {
        // Two lamps whose sample_start values are NOT index-contiguous
        // (lamp 1 starts at channel offset 9): colors must follow
        // sample_start, grid indices must follow list order.
        let layout = ControlLayout2d::new(
            Revision::new(3),
            4,
            4,
            vec![
                ControlLamp2d {
                    lamp_index: 0,
                    sample_start: 0,
                    center: [0.25, 0.25],
                    radius: 0.125,
                },
                ControlLamp2d {
                    lamp_index: 3,
                    sample_start: 9,
                    center: [0.75, 0.75],
                    radius: 0.125,
                },
            ],
        );
        let mut samples = [0u16; 12];
        samples[0..3].copy_from_slice(&[32768, 0, 0]);
        samples[9..12].copy_from_slice(&[0, 16384, 0]);
        let bytes: Vec<u8> = samples.iter().flat_map(|v| v.to_le_bytes()).collect();

        let channels = rasterize_control_preview_rgba16(
            &bytes,
            &layout,
            &ControlPreviewRasterStyle {
                radius_scale: 1.0,
                clear_color: [0.0; 4],
            },
            32,
            32,
        )
        .expect("raster");

        let pixel = |x: u32, y: u32| -> [u16; 4] {
            let base = ((y * 32 + x) * 4) as usize;
            channels[base..base + 4].try_into().expect("pixel")
        };
        assert_eq!(pixel(8, 8), [32768, 0, 0, 65535]);
        assert_eq!(pixel(24, 24), [0, 16384, 0, 65535]);
        assert_eq!(pixel(16, 16), [0, 0, 0, 0]);
    }

    #[test]
    fn lamps_past_the_sample_buffer_are_an_error() {
        let layout = ControlLayout2d::new(
            Revision::new(1),
            1,
            1,
            vec![ControlLamp2d {
                lamp_index: 7,
                sample_start: 3,
                center: [0.5, 0.5],
                radius: 0.5,
            }],
        );
        let bytes = [0u8; 8]; // four samples; lamp needs 3..6 — only 3..4 exist.
        let error = rasterize_control_preview_rgba16(
            &bytes,
            &layout,
            &ControlPreviewRasterStyle {
                radius_scale: 1.0,
                clear_color: [0.0; 4],
            },
            4,
            4,
        )
        .expect_err("out-of-range lamp");
        assert!(error.contains("sample_start"), "{error}");
    }
}
