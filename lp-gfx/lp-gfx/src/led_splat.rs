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
    /// Color the target is cleared to before splats accumulate (linear
    /// unorm-range RGBA — styling defaults live with the caller).
    pub clear_color: [f32; 4],
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
}
