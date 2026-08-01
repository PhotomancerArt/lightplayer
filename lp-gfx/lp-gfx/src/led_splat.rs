//! Inputs for the instanced LED splat op
//! ([`crate::LpGraphics::splat_leds`]): per-LED instances, shared draw
//! parameters, and the planar orthographic fit helper.
//!
//! Positions are **vec3 + view-projection** by design (planar callers pass
//! `z = 0` with an ortho fit; a future dome view passes real 3D positions
//! and an orbit camera through the same pipeline). Display positions are a
//! caller input separate from the sample positions that produced the grid —
//! the `grid_index` lane is what ties an instance to its color, so the API
//! never assumes the two layouts are equal.

/// One LED instance for [`crate::LpGraphics::splat_leds`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LedSplatInstance {
    /// World-space quad center (planar callers pass `z = 0`).
    pub position: [f32; 3],
    /// World-space splat radius (multiplied by
    /// [`LedSplatParams::radius_scale`]).
    pub radius: f32,
    /// Row-major texel index into the grid texture (see
    /// [`crate::LpShader::sample_to_grid`]); must be in bounds for the grid.
    pub grid_index: u32,
}

/// Shared draw state for one [`crate::LpGraphics::splat_leds`] pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LedSplatParams {
    /// Column-major view-projection matrix (`clip = view_proj × world`).
    /// Planar callers build it with [`ortho_fit`]; it must project clip
    /// `z` into `[0, 1]` and keep the x/y rows non-degenerate (the splat
    /// billboard axes derive from them).
    pub view_proj: [[f32; 4]; 4],
    /// Global multiplier applied to every instance radius (styling knob;
    /// `1.0` = layout radii as-is).
    pub radius_scale: f32,
    /// Global multiplier applied to every fetched grid color's RGB lanes
    /// (alpha is untouched). `1.0` = grid colors as-is. This is the
    /// display-brightness uniform: fixture previews pass
    /// `brightness / 255` here so the GPU tier shows raw sampled color ×
    /// brightness (no gamma, no color order) exactly like the CPU tier.
    pub color_scale: f32,
    /// Color the target is cleared to before splats accumulate (linear
    /// unorm-range RGBA — styling defaults live with the caller).
    pub clear_color: [f32; 4],
}

/// Inputs for [`rasterize_led_splats`], the CPU reference rasterizer.
///
/// `world_min`/`world_max` stand in for [`LedSplatParams::view_proj`]: the
/// CPU tier is planar, so the world rect that GPU callers feed [`ortho_fit`]
/// is passed directly (same y-down fit, `z` dropped). The remaining fields
/// mirror [`LedSplatParams`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LedSplatRasterParams {
    /// World-space top-left of the target (as passed to [`ortho_fit`]).
    pub world_min: [f32; 2],
    /// World-space bottom-right of the target; must exceed `world_min` on
    /// both axes.
    pub world_max: [f32; 2],
    /// See [`LedSplatParams::radius_scale`].
    pub radius_scale: f32,
    /// See [`LedSplatParams::color_scale`].
    pub color_scale: f32,
    /// See [`LedSplatParams::clear_color`].
    pub clear_color: [f32; 4],
}

/// CPU reference rasterizer for [`crate::LpGraphics::splat_leds`] — the CPU
/// preview tier's host-side fallback, and the oracle the GPU conformance
/// tests compare against.
///
/// `grid_channels` is the resident grid's content as RGBA16 channels (four
/// `u16`s per texel, row-major — what [`crate::LpGraphics::read_back`] would
/// yield); instances fetch it by `grid_index` exactly like the GPU op.
/// Returns `width × height` RGBA16 channels: per pixel,
/// `clear_color + Σ color × (1 − smoothstep(0.5, 1, d / r))` where `d` is
/// the world-space distance from the pixel center to the instance center
/// (`z` dropped) and `r = radius × radius_scale`, rounded half-up onto the
/// unorm16 grid. The GPU op accumulates in `f16`, so cross-tier comparisons
/// should allow ~130 unorm16 LSB.
pub fn rasterize_led_splats(
    grid_channels: &[u16],
    instances: &[LedSplatInstance],
    params: &LedSplatRasterParams,
    width: u32,
    height: u32,
) -> Result<alloc::vec::Vec<u16>, crate::GfxError> {
    use alloc::format;
    use alloc::string::String;
    let world_w = params.world_max[0] - params.world_min[0];
    let world_h = params.world_max[1] - params.world_min[1];
    if world_w <= 0.0 || world_h <= 0.0 || world_w.is_nan() || world_h.is_nan() {
        return Err(crate::GfxError::Backend(String::from(
            "led splat raster world rect must have positive extent",
        )));
    }
    if grid_channels.len() % 4 != 0 {
        return Err(crate::GfxError::Backend(String::from(
            "led splat raster grid channels are not RGBA16 texels",
        )));
    }
    let texel_count = (grid_channels.len() / 4) as u32;
    for instance in instances {
        if instance.grid_index >= texel_count {
            return Err(crate::GfxError::Backend(format!(
                "led splat grid_index {} is out of bounds for a {} texel grid",
                instance.grid_index, texel_count
            )));
        }
    }

    let mut accum = alloc::vec::Vec::with_capacity((width as usize) * (height as usize) * 4);
    for _ in 0..(width as u64) * u64::from(height) {
        accum.extend_from_slice(&params.clear_color);
    }
    for instance in instances {
        let base = instance.grid_index as usize * 4;
        let color = [
            f32::from(grid_channels[base]) / 65535.0 * params.color_scale,
            f32::from(grid_channels[base + 1]) / 65535.0 * params.color_scale,
            f32::from(grid_channels[base + 2]) / 65535.0 * params.color_scale,
            f32::from(grid_channels[base + 3]) / 65535.0,
        ];
        let radius = instance.radius * params.radius_scale;
        if radius <= 0.0 || radius.is_nan() {
            continue;
        }
        for py in 0..height {
            let wy = params.world_min[1] + (py as f32 + 0.5) / height as f32 * world_h;
            for px in 0..width {
                let wx = params.world_min[0] + (px as f32 + 0.5) / width as f32 * world_w;
                let dx = wx - instance.position[0];
                let dy = wy - instance.position[1];
                let d = libm::sqrtf(dx * dx + dy * dy) / radius;
                let falloff = 1.0 - smoothstep(0.5, 1.0, d);
                if falloff <= 0.0 {
                    continue;
                }
                let out = ((py * width + px) * 4) as usize;
                for (lane, &c) in color.iter().enumerate() {
                    accum[out + lane] += c * falloff;
                }
            }
        }
    }

    Ok(accum
        .into_iter()
        .map(|v| {
            let rounded = libm::floorf(v * 65535.0 + 0.5);
            if rounded <= 0.0 {
                0
            } else if rounded >= 65535.0 {
                u16::MAX
            } else {
                rounded as u16
            }
        })
        .collect())
}

/// GLSL/WGSL `smoothstep`: 0 below `edge0`, 1 above `edge1`, Hermite between.
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Column-major orthographic view-projection fitting the axis-aligned world
/// rect `min..max` to the full clip square, **y down**: `min` lands at the
/// target's top-left, `max` at its bottom-right (matching the row-0-at-top
/// convention of layout space and render targets). World `z` is dropped
/// (clip `z = 0`). `max` must exceed `min` on both axes.
#[must_use]
pub fn ortho_fit(min: [f32; 2], max: [f32; 2]) -> [[f32; 4]; 4] {
    let sx = 2.0 / (max[0] - min[0]);
    let sy = -2.0 / (max[1] - min[1]);
    let tx = -(max[0] + min[0]) / (max[0] - min[0]);
    let ty = (max[1] + min[1]) / (max[1] - min[1]);
    [
        [sx, 0.0, 0.0, 0.0],
        [0.0, sy, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        [tx, ty, 0.0, 1.0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `clip = view_proj × [world, 1]` for the column-major convention.
    fn project(view_proj: &[[f32; 4]; 4], world: [f32; 3]) -> [f32; 4] {
        let input = [world[0], world[1], world[2], 1.0];
        let mut clip = [0.0f32; 4];
        for (column, &w) in view_proj.iter().zip(&input) {
            for (lane, &c) in clip.iter_mut().zip(column) {
                *lane += c * w;
            }
        }
        clip
    }

    #[test]
    fn ortho_fit_maps_the_rect_corners_y_down() {
        let view_proj = ortho_fit([0.0, 0.0], [1.0, 1.0]);
        // Top-left of the layout is clip (-1, +1); bottom-right is (+1, -1).
        assert_eq!(project(&view_proj, [0.0, 0.0, 0.0]), [-1.0, 1.0, 0.0, 1.0]);
        assert_eq!(project(&view_proj, [1.0, 1.0, 0.0]), [1.0, -1.0, 0.0, 1.0]);
        assert_eq!(project(&view_proj, [0.5, 0.5, 0.0]), [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn ortho_fit_drops_world_z_and_honors_offset_rects() {
        let view_proj = ortho_fit([2.0, -1.0], [6.0, 1.0]);
        assert_eq!(project(&view_proj, [2.0, -1.0, 0.0]), [-1.0, 1.0, 0.0, 1.0]);
        assert_eq!(project(&view_proj, [6.0, 1.0, 0.0]), [1.0, -1.0, 0.0, 1.0]);
        // z is dropped: the projection is planar.
        assert_eq!(project(&view_proj, [4.0, 0.0, 7.5]), [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn raster_places_grid_colors_at_instance_centers_over_the_clear_color() {
        // 2-texel grid: half red, quarter green (alpha full).
        let grid = [32768u16, 0, 0, 65535, 0, 16384, 0, 65535];
        let instances = [
            LedSplatInstance {
                position: [0.25, 0.25, 0.0],
                radius: 0.125,
                grid_index: 0,
            },
            // Nonzero z: the planar raster drops it like ortho_fit does.
            LedSplatInstance {
                position: [0.75, 0.75, 3.0],
                radius: 0.125,
                grid_index: 1,
            },
        ];
        let channels =
            rasterize_led_splats(&grid, &instances, &raster_params(), 32, 32).expect("raster");

        // Instance centers carry their full grid color (solid falloff core).
        assert_eq!(pixel(&channels, 32, 8, 8), [32768, 0, 0, 65535]);
        assert_eq!(pixel(&channels, 32, 24, 24), [0, 16384, 0, 65535]);
        // The midpoint between them stays at the clear color.
        assert_eq!(pixel(&channels, 32, 16, 16), [0, 0, 0, 0]);
    }

    #[test]
    fn raster_accumulates_coincident_splats_additively() {
        let grid = [16384u16, 0, 0, 65535];
        let instance = LedSplatInstance {
            position: [0.5, 0.5, 0.0],
            radius: 0.25,
            grid_index: 0,
        };
        let single = rasterize_led_splats(&grid, &[instance], &raster_params(), 32, 32)
            .expect("single raster");
        let double = rasterize_led_splats(&grid, &[instance, instance], &raster_params(), 32, 32)
            .expect("double raster");

        assert_eq!(pixel(&single, 32, 16, 16)[0], 16384);
        assert_eq!(pixel(&double, 32, 16, 16)[0], 32768);
    }

    #[test]
    fn raster_clear_color_fills_empty_targets_rounded_half_up() {
        let params = LedSplatRasterParams {
            clear_color: [0.1, 0.2, 0.3, 0.4],
            ..raster_params()
        };
        let channels = rasterize_led_splats(&[], &[], &params, 4, 4).expect("raster");
        assert_eq!(pixel(&channels, 4, 1, 2), [6554, 13107, 19661, 26214]);
    }

    #[test]
    fn raster_color_scale_multiplies_rgb_but_not_alpha() {
        let grid = [40000u16, 20000, 10000, 60000];
        let instance = LedSplatInstance {
            position: [0.5, 0.5, 0.0],
            radius: 0.5,
            grid_index: 0,
        };
        let params = LedSplatRasterParams {
            color_scale: 0.5,
            ..raster_params()
        };
        let channels = rasterize_led_splats(&grid, &[instance], &params, 8, 8).expect("raster");
        assert_eq!(pixel(&channels, 8, 4, 4), [20000, 10000, 5000, 60000]);
    }

    #[test]
    fn raster_rejects_out_of_bounds_grid_indices() {
        let grid = [0u16, 0, 0, 0];
        let instances = [LedSplatInstance {
            position: [0.5, 0.5, 0.0],
            radius: 0.5,
            grid_index: 1,
        }];
        match rasterize_led_splats(&grid, &instances, &raster_params(), 4, 4) {
            Err(crate::GfxError::Backend(message)) => {
                assert!(message.contains("grid_index"), "{message}");
            }
            other => panic!("expected out-of-bounds error, got {other:?}"),
        }
    }

    fn raster_params() -> LedSplatRasterParams {
        LedSplatRasterParams {
            world_min: [0.0, 0.0],
            world_max: [1.0, 1.0],
            radius_scale: 1.0,
            color_scale: 1.0,
            clear_color: [0.0, 0.0, 0.0, 0.0],
        }
    }

    fn pixel(channels: &[u16], width: u32, x: u32, y: u32) -> [u16; 4] {
        let base = ((y * width + x) * 4) as usize;
        channels[base..base + 4].try_into().expect("pixel channels")
    }
}
