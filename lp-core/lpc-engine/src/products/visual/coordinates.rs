//! Coordinate-space helpers for visual products.
//!
//! LightPlayer uses three visual coordinate spaces:
//!
//! - Fixture UV: authored device/layout positions normalized to `[0, 1]`.
//! - Shader pixel space: continuous pixel coordinates passed to `render(vec2 pos)`,
//!   with pixel centers at `x + 0.5`, `y + 0.5`.
//! - Texture UV: normalized coordinates used to sample a materialized texture.
//!
//! The direct-sampling buffer (`lp_gfx::SamplePointsHandle`) carries shader pixel-space
//! coordinates encoded as Q16.16 integers. Texture sample batches carry texture UV
//! coordinates encoded as Q16.16 integers.
//!
//! # The projection library
//!
//! When producer and consumer live in different spaces, the mismatch is
//! resolved by exactly one thing: **a coordinate map from the sampling
//! space into the product's space**, `fn(target_coord) -> source_coord`
//! (vision D5 — "every mismatch cell is a coordinate map"). Radial,
//! angular, extrude and mirror are the 2D→1D-source cells;
//! [`centre_scanline`] is the 1D→2D-source one. They are pure functions on
//! normalized `[0, 1]` coordinates so the same math serves the CPU sample
//! path, the texture-fill path, and (later) an explicit projection node.
//!
//! All of them run in `f32` via `libm` — never `std` float methods, which
//! do not exist on the firmware tiers.

pub const Q16_ONE: i32 = 1 << 16;

/// The radial map's normalization constant: **corner reach = 1**.
///
/// `radial` measures the distance from the centre of the unit square, so
/// its natural maximum is the corner distance `|(0.5, 0.5)| = √2/2 ≈
/// 0.7071`. Dividing by that constant makes the map reach exactly `t = 1`
/// in the corners and `t ≈ 0.707` at the edge midpoints — i.e. the whole
/// strip is visible on a square surface and nothing clips.
///
/// The alternative anchor (edge reach = 1, divide by 0.5) hides the last
/// 29% of the strip in the corners, and the UX spike's `/0.66` was an
/// eyeballed compromise between the two. Corner reach is the one that can
/// be stated as a rule instead of a taste, so it is the one we keep.
pub const RADIAL_CORNER_REACH: f32 = core::f32::consts::SQRT_2 / 2.0;

/// Extrude: the strip runs along x, every row identical (the system
/// default for a 1D source on a 2D surface).
#[must_use]
pub fn extrude(u: f32, _v: f32) -> f32 {
    u.clamp(0.0, 1.0)
}

/// Radial: distance from the centre, normalized so the corners reach 1
/// ([`RADIAL_CORNER_REACH`]).
#[must_use]
pub fn radial(u: f32, v: f32) -> f32 {
    let dx = u - 0.5;
    let dy = v - 0.5;
    let distance = libm::sqrtf(dx * dx + dy * dy);
    (distance / RADIAL_CORNER_REACH).clamp(0.0, 1.0)
}

/// Angular: the angle around the centre, mapped to `[0, 1)` counter-
/// clockwise from the +x axis. The centre point itself reads 0.
#[must_use]
pub fn angular(u: f32, v: f32) -> f32 {
    let dx = u - 0.5;
    let dy = v - 0.5;
    if dx == 0.0 && dy == 0.0 {
        return 0.0;
    }
    let turns = libm::atan2f(dy, dx) / core::f32::consts::TAU;
    // atan2 is (-0.5, 0.5] turns; wrap the negative half up into [0, 1).
    let wrapped = if turns < 0.0 { turns + 1.0 } else { turns };
    // A hair below 1.0 can round to 1.0; keep the range half-open.
    if wrapped >= 1.0 { 0.0 } else { wrapped }
}

/// Mirror: the strip runs out from the centre column in both directions.
#[must_use]
pub fn mirror(u: f32, _v: f32) -> f32 {
    libm::fabsf(2.0 * u.clamp(0.0, 1.0) - 1.0)
}

/// The 2D→1D direction: a 1D sampling coordinate lands on the centre
/// scanline of a 2D source (vision D8, the only cell authorable today).
#[must_use]
pub fn centre_scanline(t: f32) -> (f32, f32) {
    (t.clamp(0.0, 1.0), 0.5)
}

/// The strip coordinate a
/// [`ProjectionDirection`](crate::products::visual::ProjectionDirection)
/// picks out of a normalized `(u, v)` — the ONE place direction enters the
/// math (G1b ruling 4): the directional shapes run their existing 1-axis
/// map over this coordinate. `Right` is `u` verbatim, so the default
/// direction is bit-for-bit the pre-directional behavior.
#[must_use]
pub fn directed_coord(
    direction: crate::products::visual::ProjectionDirection,
    u: f32,
    v: f32,
) -> f32 {
    use crate::products::visual::ProjectionDirection;
    match direction {
        ProjectionDirection::Right => u,
        ProjectionDirection::Left => 1.0 - u,
        ProjectionDirection::Down => v,
        ProjectionDirection::Up => 1.0 - v,
    }
}

/// The strip coordinate a mirror FOLD produces from a normalized
/// `(u, v)` (mirror-direction ruling): outward runs the strip from the
/// centre line toward both edges (`|2s−1|` — the pre-direction mirror,
/// verbatim, for `OutwardX`), inward from both edges toward the centre
/// (`1 − |2s−1|`); the axis picks `x` or `y` as the folded coordinate.
#[must_use]
pub fn mirror_fold_coord(
    direction: crate::products::visual::MirrorDirection,
    u: f32,
    v: f32,
) -> f32 {
    use crate::products::visual::MirrorDirection;
    match direction {
        MirrorDirection::OutwardX => mirror(u, v),
        MirrorDirection::InwardX => 1.0 - mirror(u, v),
        MirrorDirection::OutwardY => mirror(v, u),
        MirrorDirection::InwardY => 1.0 - mirror(v, u),
    }
}

/// The strip coordinate a radial FLIP produces (radial/angular flip
/// ruling): `Outward` is the pre-flip radial verbatim (`u = r`), `Inward`
/// runs the strip edge → centre (`u = 1 − r`).
#[must_use]
pub fn radial_flip_coord(
    direction: crate::products::visual::RadialDirection,
    u: f32,
    v: f32,
) -> f32 {
    use crate::products::visual::RadialDirection;
    match direction {
        RadialDirection::Outward => radial(u, v),
        RadialDirection::Inward => 1.0 - radial(u, v),
    }
}

/// The strip coordinate an angular SWEEP produces (radial/angular flip
/// ruling): `Clockwise` is the pre-flip sweep verbatim, `CounterClockwise`
/// negates it (`u = 1 − a`, wrapped so the range stays half-open — the
/// seam stays at the same angle, only the direction of travel flips).
#[must_use]
pub fn angular_sweep_coord(
    direction: crate::products::visual::AngularDirection,
    u: f32,
    v: f32,
) -> f32 {
    use crate::products::visual::AngularDirection;
    match direction {
        AngularDirection::Clockwise => angular(u, v),
        AngularDirection::CounterClockwise => {
            let swept = 1.0 - angular(u, v);
            if swept >= 1.0 { 0.0 } else { swept }
        }
    }
}

/// Apply a [`CellProjection`](crate::products::visual::CellProjection) as a
/// normalized target→source map.
#[must_use]
pub fn project_2d_to_1d(cell: crate::products::visual::CellProjection, u: f32, v: f32) -> f32 {
    use crate::products::visual::CellProjection;
    match cell {
        CellProjection::Extrude(direction) => extrude(directed_coord(direction, u, v), v),
        CellProjection::Radial(direction) => radial_flip_coord(direction, u, v),
        CellProjection::Angular(direction) => angular_sweep_coord(direction, u, v),
        CellProjection::Mirror(direction) => mirror_fold_coord(direction, u, v),
    }
}

#[must_use]
pub fn normalized_f32_to_q16(value: f32) -> i32 {
    let clamped = value.clamp(0.0, 1.0);
    (clamped * Q16_ONE as f32) as i32
}

#[must_use]
pub fn normalized_q16_to_pixel_q16(value: i32, extent: u32) -> i32 {
    let scaled = i64::from(value) * i64::from(extent);
    scaled.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

#[must_use]
pub fn pixel_q16_to_normalized_q16(coord: i32, extent: u32) -> i32 {
    if extent == 0 {
        return 0;
    }
    let normalized = i64::from(coord) / i64::from(extent);
    normalized.clamp(0, i64::from(Q16_ONE - 1)) as i32
}

#[must_use]
pub fn texel_center_to_uv_q16(texel: u32, extent: u32) -> i32 {
    if extent == 0 {
        return 0;
    }
    (((u64::from(texel)) * Q16_ONE as u64 + (Q16_ONE as u64 / 2)) / u64::from(extent)) as i32
}

#[must_use]
pub fn texture_uv_q16_to_texel(value: i32, extent: u32) -> u32 {
    if extent == 0 || value <= 0 {
        return 0;
    }
    let scaled = ((i64::from(value)) * i64::from(extent)) >> 16;
    u32::try_from(scaled).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_q16_scales_to_shader_pixel_space() {
        assert_eq!(normalized_q16_to_pixel_q16(0, 16), 0);
        assert_eq!(normalized_q16_to_pixel_q16(32768, 16), 8 * Q16_ONE);
        assert_eq!(normalized_q16_to_pixel_q16(Q16_ONE, 16), 16 * Q16_ONE);
    }

    #[test]
    fn pixel_q16_scales_to_normalized_texture_space() {
        assert_eq!(pixel_q16_to_normalized_q16(0, 16), 0);
        assert_eq!(pixel_q16_to_normalized_q16(8 * Q16_ONE, 16), 32768);
        assert_eq!(pixel_q16_to_normalized_q16(16 * Q16_ONE, 16), 65535);
    }

    /// Q16.16 round-tripping loses at most 1/65536, and the maps
    /// themselves are exact rational/`libm` evaluations — 1e-5 is well
    /// inside both float modes' agreement on these operations.
    const EPS: f32 = 1e-5;

    fn assert_close(actual: f32, expected: f32, what: &str) {
        assert!(
            libm::fabsf(actual - expected) <= EPS,
            "{what}: {actual} != {expected}"
        );
    }

    #[test]
    fn extrude_is_the_identity_on_u_at_every_corner() {
        for v in [0.0, 0.5, 1.0] {
            assert_close(extrude(0.0, v), 0.0, "left edge");
            assert_close(extrude(0.25, v), 0.25, "quarter");
            assert_close(extrude(1.0, v), 1.0, "right edge");
        }
    }

    #[test]
    fn radial_reaches_one_in_the_corners_and_zero_at_the_centre() {
        assert_close(radial(0.5, 0.5), 0.0, "centre");
        for (u, v) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
            assert_close(radial(u, v), 1.0, "corner");
        }
        // Edge midpoints sit at half the corner distance / corner reach.
        let edge = 0.5 / RADIAL_CORNER_REACH;
        assert_close(radial(0.0, 0.5), edge, "left edge midpoint");
        assert_close(radial(0.5, 1.0), edge, "bottom edge midpoint");
        assert_close(edge, core::f32::consts::SQRT_2 / 2.0, "edge reach is √2/2");
    }

    #[test]
    fn angular_runs_one_turn_counter_clockwise_from_plus_x() {
        assert_close(angular(1.0, 0.5), 0.0, "+x");
        assert_close(angular(0.5, 1.0), 0.25, "+y");
        assert_close(angular(0.0, 0.5), 0.5, "-x");
        assert_close(angular(0.5, 0.0), 0.75, "-y");
        assert_close(angular(0.5, 0.5), 0.0, "centre is 0 by convention");
        for (u, v) in [(0.0, 0.0), (1.0, 1.0), (0.25, 0.75)] {
            let t = angular(u, v);
            assert!((0.0..1.0).contains(&t), "angular stays in [0, 1): {t}");
        }
    }

    #[test]
    fn mirror_folds_the_strip_around_the_centre_column() {
        assert_close(mirror(0.0, 0.3), 1.0, "left edge");
        assert_close(mirror(0.25, 0.3), 0.5, "quarter");
        assert_close(mirror(0.5, 0.3), 0.0, "centre column");
        assert_close(mirror(0.75, 0.3), 0.5, "three quarters");
        assert_close(mirror(1.0, 0.3), 1.0, "right edge");
    }

    #[test]
    fn centre_scanline_puts_every_point_on_the_middle_row() {
        for t in [0.0, 0.25, 0.5, 1.0] {
            let (u, v) = centre_scanline(t);
            assert_close(u, t, "u passes through");
            assert_close(v, 0.5, "v is the centre row");
        }
    }

    #[test]
    fn the_cell_dispatcher_matches_the_named_maps() {
        use crate::products::visual::{
            AngularDirection, CellProjection, MirrorDirection, ProjectionDirection, RadialDirection,
        };
        for (u, v) in [(0.0, 0.0), (0.3, 0.7), (1.0, 1.0)] {
            assert_close(
                project_2d_to_1d(CellProjection::Extrude(ProjectionDirection::Right), u, v),
                extrude(u, v),
                "extrude",
            );
            assert_close(
                project_2d_to_1d(CellProjection::Radial(RadialDirection::Outward), u, v),
                radial(u, v),
                "radial (outward IS the pre-flip behavior)",
            );
            assert_close(
                project_2d_to_1d(CellProjection::Angular(AngularDirection::Clockwise), u, v),
                angular(u, v),
                "angular (clockwise IS the pre-flip behavior)",
            );
            assert_close(
                project_2d_to_1d(CellProjection::Mirror(MirrorDirection::OutwardX), u, v),
                mirror(u, v),
                "mirror (outward-x IS the pre-direction behavior)",
            );
        }
    }

    /// Direction picks the strip coordinate and nothing else (G1b ruling
    /// 4): `Right` is `u` verbatim (today's behavior), `Left` reverses it,
    /// `Down`/`Up` run the rows instead of the columns.
    #[test]
    fn directed_coord_picks_the_strip_axis() {
        use crate::products::visual::ProjectionDirection;
        let (u, v) = (0.25, 0.7);
        assert_close(directed_coord(ProjectionDirection::Right, u, v), 0.25, "→");
        assert_close(directed_coord(ProjectionDirection::Left, u, v), 0.75, "←");
        assert_close(directed_coord(ProjectionDirection::Down, u, v), 0.7, "↓");
        assert_close(
            directed_coord(ProjectionDirection::Up, u, v),
            0.3,
            "↑ is 1 − y",
        );
    }

    /// The radial flip runs the strip edge → centre: 1 at the centre,
    /// 0 in the corners — `1 − r` everywhere.
    #[test]
    fn radial_inward_runs_the_strip_edge_to_centre() {
        use crate::products::visual::{CellProjection, RadialDirection};
        assert_close(
            project_2d_to_1d(CellProjection::Radial(RadialDirection::Inward), 0.5, 0.5),
            1.0,
            "inward centre",
        );
        for (u, v) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
            assert_close(
                project_2d_to_1d(CellProjection::Radial(RadialDirection::Inward), u, v),
                0.0,
                "inward corner",
            );
        }
        for (u, v) in [(0.3, 0.7), (0.9, 0.2)] {
            assert_close(
                project_2d_to_1d(CellProjection::Radial(RadialDirection::Inward), u, v),
                1.0 - radial(u, v),
                "inward is 1 − r",
            );
        }
    }

    /// The angular flip negates the sweep — `1 − a`, wrapped so the range
    /// stays half-open (the seam stays put; only the travel flips).
    #[test]
    fn angular_counter_clockwise_negates_the_sweep() {
        use crate::products::visual::{AngularDirection, CellProjection};
        let ccw = |u, v| {
            project_2d_to_1d(
                CellProjection::Angular(AngularDirection::CounterClockwise),
                u,
                v,
            )
        };
        assert_close(ccw(1.0, 0.5), 0.0, "+x stays the seam (wrapped)");
        assert_close(ccw(0.5, 1.0), 0.75, "+y");
        assert_close(ccw(0.0, 0.5), 0.5, "-x");
        assert_close(ccw(0.5, 0.0), 0.25, "-y");
        for (u, v) in [(0.0, 0.0), (1.0, 1.0), (0.25, 0.75)] {
            let t = ccw(u, v);
            assert!((0.0..1.0).contains(&t), "ccw stays in [0, 1): {t}");
        }
    }

    /// The directional dispatcher arms compose direction with the existing
    /// shape math — extrude-left is the reversed ramp; each mirror fold is
    /// its sense × axis over the same `|2s−1|` core.
    #[test]
    fn directional_arms_compose_direction_with_the_shape_math() {
        use crate::products::visual::{CellProjection, MirrorDirection, ProjectionDirection};
        for (u, v) in [(0.0, 0.0), (0.3, 0.7), (1.0, 1.0)] {
            assert_close(
                project_2d_to_1d(CellProjection::Extrude(ProjectionDirection::Left), u, v),
                extrude(1.0 - u, v),
                "extrude ←",
            );
            assert_close(
                project_2d_to_1d(CellProjection::Extrude(ProjectionDirection::Up), u, v),
                extrude(1.0 - v, u),
                "extrude ↑",
            );
            assert_close(
                project_2d_to_1d(CellProjection::Mirror(MirrorDirection::InwardX), u, v),
                1.0 - mirror(u, v),
                "mirror →← (inward-x)",
            );
            assert_close(
                project_2d_to_1d(CellProjection::Mirror(MirrorDirection::OutwardY), u, v),
                mirror(v, u),
                "mirror ↑↓ (outward-y)",
            );
            assert_close(
                project_2d_to_1d(CellProjection::Mirror(MirrorDirection::InwardY), u, v),
                1.0 - mirror(v, u),
                "mirror ↓↑ (inward-y)",
            );
        }
    }

    #[test]
    fn texel_center_scales_each_axis_by_its_own_extent() {
        assert_eq!(texel_center_to_uv_q16(0, 2), 16384);
        assert_eq!(texel_center_to_uv_q16(1, 2), 49152);
        assert_eq!(texel_center_to_uv_q16(2, 4), 40960);
    }
}
