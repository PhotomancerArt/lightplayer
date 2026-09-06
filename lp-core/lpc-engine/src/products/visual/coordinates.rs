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

/// The strip coordinate a base [`ProjectionShape`]
/// (`crate::products::visual::ProjectionShape`) reads off a normalized
/// `(u, v)` — the first link of THE uniform chain (the factorization
/// ruling): `ExtrudeX` is `u` verbatim, `ExtrudeY` is `v`, and
/// radial/angular are the existing centre maps.
#[must_use]
pub fn shape_coord(shape: crate::products::visual::ProjectionShape, u: f32, v: f32) -> f32 {
    use crate::products::visual::ProjectionShape;
    match shape {
        ProjectionShape::ExtrudeX => extrude(u, v),
        ProjectionShape::ExtrudeY => extrude(v, u),
        ProjectionShape::Radial => radial(u, v),
        ProjectionShape::Angular => angular(u, v),
    }
}

/// Apply a [`CellProjection`](crate::products::visual::CellProjection) as a
/// normalized target→source map — ONE uniform transform chain for every
/// factored cell (the factorization ruling replaced the per-shape arms):
///
/// ```text
/// t = shape_coord(shape, u, v);
/// if mirror { t = 1 − |2t − 1| }   // fold around the midpoint
/// if flip   { t = 1 − t }          // reverse the strip
/// ```
///
/// A non-mirrored angular's output is CYCLIC with a discontinuous seam,
/// so its range is kept half-open after the chain (a flipped seam value
/// of exactly `1.0` wraps to `0.0`) — the seam stays at the same angle,
/// only the direction of travel changes; bit-identical to the
/// pre-factored counter-clockwise arm. A MIRRORED angular is continuous
/// across the seam (the fold runs up and back), so `1.0` there is the
/// fold's genuine far end and is never wrapped.
#[must_use]
pub fn project_2d_to_1d(cell: crate::products::visual::CellProjection, u: f32, v: f32) -> f32 {
    use crate::products::visual::ProjectionShape;
    let mut t = shape_coord(cell.shape, u, v);
    if cell.mirror {
        t = 1.0 - libm::fabsf(2.0 * t - 1.0);
    }
    if cell.flip {
        t = 1.0 - t;
    }
    if cell.shape == ProjectionShape::Angular && !cell.mirror && t >= 1.0 {
        t = 0.0;
    }
    t
}

/// Normalized `[0, 1]` f32 → Q16.16, truncating: exactly
/// `(value.clamp(0.0, 1.0) * 65536.0) as i32` for every f32 bit pattern
/// (negative and −0.0 → 0, NaN → 0, ≥ 1.0 and +∞ → 65536, subnormals → 0),
/// computed on the bit pattern.
///
/// The float form is four libcalls on a part with no FPU (two compares, a
/// multiply, a float-to-int), and since bounded sample batches
/// (`docs/adr/2026-09-06-direct-sampling-bounded-batches.md`) a Direct
/// fixture converts every lamp's centre on every render. This is ~15
/// integer ops instead. `q16_conversion_matches_the_float_reference_exhaustively`
/// is the proof, over every non-negative bit pattern.
#[must_use]
pub fn normalized_f32_to_q16(value: f32) -> i32 {
    let bits = value.to_bits();
    if bits & 0x8000_0000 != 0 {
        // Negative, including −0.0: the clamp answers 0.0 (or keeps −0.0,
        // which the cast also takes to 0).
        return 0;
    }
    if bits > 0x7F80_0000 {
        // NaN: `clamp` passes it through and the saturating cast gives 0.
        return 0;
    }
    if bits >= 0x3F80_0000 {
        // ≥ 1.0, including +∞: the clamp answers 1.0.
        return Q16_ONE;
    }
    let exponent = (bits >> 23) as i32;
    if exponent == 0 {
        // Subnormal: below 2^-126, so × 2^16 is far below 1.
        return 0;
    }
    // value = significand × 2^(exponent − 127 − 23); value × 2^16 is
    // significand × 2^(exponent − 134), and truncation is a right shift.
    let significand = (bits & 0x007F_FFFF) | 0x0080_0000;
    let shift = 134 - exponent;
    if shift >= 32 {
        0
    } else {
        (significand >> shift) as i32
    }
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

    /// THE bit-identity table (factorization ruling): every pre-factored
    /// vocabulary entry re-expressed as shape × mirror × flip must equal
    /// the same named map it always ran. Extrude-Right = ExtrudeX plain;
    /// Left = ExtrudeX + flip; Down/Up = ExtrudeY (± flip); the four
    /// mirror folds = ExtrudeX|Y × mirror (× flip — outward is the
    /// mirrored fold REVERSED, `|2s−1|`); radial inward = Radial + flip;
    /// angular counter-clockwise = Angular + flip.
    #[test]
    fn the_factored_chain_is_bit_identical_to_the_named_maps() {
        use crate::products::visual::{CellProjection, ProjectionShape};
        let cell = |shape, m, f| CellProjection {
            shape,
            mirror: m,
            flip: f,
        };
        for (u, v) in [(0.0, 0.0), (0.3, 0.7), (1.0, 1.0), (0.9, 0.2)] {
            // extrude directions
            assert_close(
                project_2d_to_1d(cell(ProjectionShape::ExtrudeX, false, false), u, v),
                extrude(u, v),
                "extrude right = extrude-x",
            );
            assert_close(
                project_2d_to_1d(cell(ProjectionShape::ExtrudeX, false, true), u, v),
                extrude(1.0 - u, v),
                "extrude left = extrude-x + flip",
            );
            assert_close(
                project_2d_to_1d(cell(ProjectionShape::ExtrudeY, false, false), u, v),
                extrude(v, u),
                "extrude down = extrude-y",
            );
            assert_close(
                project_2d_to_1d(cell(ProjectionShape::ExtrudeY, false, true), u, v),
                extrude(1.0 - v, u),
                "extrude up = extrude-y + flip",
            );
            // mirror folds (outward = the pre-direction mirror |2s−1|)
            assert_close(
                project_2d_to_1d(cell(ProjectionShape::ExtrudeX, true, true), u, v),
                mirror(u, v),
                "mirror outward-x = extrude-x + mirror + flip",
            );
            assert_close(
                project_2d_to_1d(cell(ProjectionShape::ExtrudeX, true, false), u, v),
                1.0 - mirror(u, v),
                "mirror inward-x = extrude-x + mirror",
            );
            assert_close(
                project_2d_to_1d(cell(ProjectionShape::ExtrudeY, true, true), u, v),
                mirror(v, u),
                "mirror outward-y = extrude-y + mirror + flip",
            );
            assert_close(
                project_2d_to_1d(cell(ProjectionShape::ExtrudeY, true, false), u, v),
                1.0 - mirror(v, u),
                "mirror inward-y = extrude-y + mirror",
            );
            // radial flips
            assert_close(
                project_2d_to_1d(cell(ProjectionShape::Radial, false, false), u, v),
                radial(u, v),
                "radial outward = radial",
            );
            assert_close(
                project_2d_to_1d(cell(ProjectionShape::Radial, false, true), u, v),
                1.0 - radial(u, v),
                "radial inward = radial + flip",
            );
            // angular sweep (the wrap case is asserted separately below)
            assert_close(
                project_2d_to_1d(cell(ProjectionShape::Angular, false, false), u, v),
                angular(u, v),
                "angular clockwise = angular",
            );
        }
    }

    /// The angular flip negates the sweep — `1 − a`, wrapped so the range
    /// stays half-open (the seam stays put; only the travel flips) —
    /// bit-identical to the pre-factored counter-clockwise arm.
    #[test]
    fn angular_flip_negates_the_sweep_and_stays_half_open() {
        use crate::products::visual::{CellProjection, ProjectionShape};
        let ccw = |u, v| {
            project_2d_to_1d(
                CellProjection {
                    shape: ProjectionShape::Angular,
                    mirror: false,
                    flip: true,
                },
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

    /// The NEW states the factorization makes reachable (Yona's ask):
    /// angular + mirror is the up-and-back sweep — the strip runs half a
    /// turn out and folds back over the other half — and radial + mirror
    /// is a ring ramp that peaks mid-radius.
    #[test]
    fn the_new_mirror_composites_run_up_and_back() {
        use crate::products::visual::{CellProjection, ProjectionShape};
        let am = |u, v| {
            project_2d_to_1d(
                CellProjection {
                    shape: ProjectionShape::Angular,
                    mirror: true,
                    flip: false,
                },
                u,
                v,
            )
        };
        // The sweep runs 0→1 over the first half turn and back 1→0 over
        // the second: quarter turns land on the same value.
        assert_close(am(1.0, 0.5), 0.0, "+x start");
        assert_close(am(0.0, 0.5), 1.0, "-x is the far end of the fold");
        assert_close(am(0.5, 1.0), 0.5, "+y quarter");
        assert_close(am(0.5, 0.0), 0.5, "-y quarter matches +y — up and back");

        let rm = |u, v| {
            project_2d_to_1d(
                CellProjection {
                    shape: ProjectionShape::Radial,
                    mirror: true,
                    flip: false,
                },
                u,
                v,
            )
        };
        assert_close(rm(0.5, 0.5), 0.0, "centre = fold start");
        for (u, v) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
            assert_close(rm(u, v), 0.0, "corners fold back to 0");
        }
        for (u, v) in [(0.3, 0.7), (0.9, 0.2)] {
            assert_close(
                rm(u, v),
                1.0 - libm::fabsf(2.0 * radial(u, v) - 1.0),
                "mid fold",
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

/// `normalized_f32_to_q16` is integer arithmetic standing in for a float
/// expression; these pin that the two never disagree.
#[cfg(test)]
mod q16_conversion_tests {
    use super::{Q16_ONE, normalized_f32_to_q16};

    /// The float expression the integer form must reproduce bit for bit.
    fn reference(value: f32) -> i32 {
        (value.clamp(0.0, 1.0) * Q16_ONE as f32) as i32
    }

    fn check(bits: u32) {
        let value = f32::from_bits(bits);
        assert_eq!(
            normalized_f32_to_q16(value),
            reference(value),
            "bits {bits:#010x} ({value:?})"
        );
    }

    /// Every non-negative bit pattern (finite, +∞, every positive NaN), plus
    /// the negative half at a coarse stride — about 2.1 billion checks; run
    /// in release once per change to the function:
    ///
    /// ```bash
    /// cargo test -p lpc-engine --release q16_conversion_matches_the_float_reference_exhaustively -- --ignored
    /// ```
    #[test]
    #[ignore = "2^31 patterns — release-mode, run explicitly"]
    fn q16_conversion_matches_the_float_reference_exhaustively() {
        for bits in 0..=0x7FFF_FFFFu32 {
            check(bits);
        }
        for bits in (0x8000_0000u32..=0xFFFF_FFFF).step_by(4_099) {
            check(bits);
        }
        check(0xFFFF_FFFF);
    }

    /// The default-run sample: a stride over the same ranges plus the edges
    /// that matter — the clamp boundaries, the subnormal/normal boundary,
    /// the last value below 1.0, NaN, the signed zeros and infinities.
    #[test]
    fn q16_conversion_matches_the_float_reference_sampled() {
        for bits in (0..=0x7FFF_FFFFu32).step_by(1_009) {
            check(bits);
        }
        for bits in (0x8000_0000u32..=0xFFFF_FFFF).step_by(100_003) {
            check(bits);
        }
        for value in [
            0.0f32,
            -0.0,
            1.0,
            -1.0,
            0.5,
            0.25,
            0.75,
            0.999_999_94, // the last f32 below 1.0
            1.000_000_1,
            2.0,
            65_535.0 / 65_536.0,
            1.0 / 65_536.0,
            1.5 / 65_536.0,
            f32::MIN_POSITIVE,       // 2^-126, the smallest normal
            f32::MIN_POSITIVE / 2.0, // subnormal
            1.401_298_5e-45,         // 2^-149, the smallest subnormal
            f32::EPSILON,
            f32::MAX,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
            -f32::NAN,
        ] {
            check(value.to_bits());
        }
        assert_eq!(normalized_f32_to_q16(1.0), Q16_ONE);
        assert_eq!(normalized_f32_to_q16(0.5), Q16_ONE / 2);
        assert_eq!(normalized_f32_to_q16(0.999_999_94), Q16_ONE - 1);
    }
}
