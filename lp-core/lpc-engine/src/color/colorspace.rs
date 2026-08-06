//! Authoring space → canonical LinearSrgb, and interpolation *inside* an
//! authoring space.
//!
//! Two operations, in the order `docs/design/color.md` §7 mandates:
//! **interpolate first, in the gradient's own space; convert second.** An
//! Oklch rainbow that is converted before interpolation is just an sRGB
//! rainbow with extra steps — the ordering is the whole reason non-canonical
//! authoring spaces exist.
//!
//! # Coordinate conventions
//!
//! `GradientStop::c` is three numbers whose meaning is the stop's
//! [`Colorspace`]. There is exactly one non-fractional unit in the set:
//!
//! | Space        | `c[0]`      | `c[1]`         | `c[2]`         |
//! |--------------|-------------|----------------|----------------|
//! | `LinearSrgb` | R `[0,1]`   | G `[0,1]`      | B `[0,1]`      |
//! | `Srgb`       | R' `[0,1]`  | G' `[0,1]`     | B' `[0,1]`     |
//! | `Hsl`        | H **deg**   | S `[0,1]`      | L `[0,1]`      |
//! | `Hsv`        | H **deg**   | S `[0,1]`      | V `[0,1]`      |
//! | `Oklab`      | L `[0,1]`   | a `~[-.4,.4]`  | b `~[-.4,.4]`  |
//! | `Oklch`      | L `[0,1]`   | C `~[0,0.4]`   | H **deg**      |
//!
//! **Hue is degrees**, in every cylindrical space, matching CSS
//! (`hsl(120deg …)`, `oklch(… 120)`) — which is what a color picker, a
//! pasted CSS value, and every palette-import source already speak. Nothing
//! else here is an angle, so "the only unit that is not a `0..1` fraction is
//! a hue angle, and it is degrees" is the whole rule.
//!
//! Coordinates outside those ranges are legal and meaningful (`color.md` §10
//! rule 6): hues wrap, and out-of-gamut/boosted values survive until the
//! texture-write boundary clamps them to the Unorm16 grid.
//!
//! # `libm`, not `f32` methods
//!
//! `powf`/`sinf`/`cosf` are `std`; this crate compiles for every firmware
//! tier. Same rule the phasor evaluator follows.

use lpc_model::{Colorspace, InterpMethod};

/// Convert one stop's coordinates from `space` to canonical LinearSrgb.
///
/// The result is **not** clamped: `color.md` §10 rule 6 keeps overshoot
/// meaningful all the way to the storage boundary, which is the one place
/// that has to give up range.
#[must_use]
pub fn to_linear_srgb(space: Colorspace, c: [f32; 3]) -> [f32; 3] {
    match space {
        Colorspace::LinearSrgb => c,
        Colorspace::Srgb => [
            srgb_to_linear(c[0]),
            srgb_to_linear(c[1]),
            srgb_to_linear(c[2]),
        ],
        Colorspace::Hsl => srgb_triple_to_linear(hsl_to_srgb(c)),
        Colorspace::Hsv => srgb_triple_to_linear(hsv_to_srgb(c)),
        Colorspace::Oklab => oklab_to_linear_srgb(c),
        Colorspace::Oklch => oklab_to_linear_srgb(oklch_to_oklab(c)),
    }
}

/// Interpolate between two coordinate triples **within** `space`, under
/// `method`.
///
/// `t` is the already-normalized position between the two stops; it is
/// clamped to `[0,1]` so a caller's rounding can never extrapolate a stop
/// segment.
///
/// Cylindrical spaces take the **shortest arc** through their hue: that is
/// what makes an Oklch red→blue read as a rainbow in one direction rather
/// than a 300° sweep through green. A caller wanting the long way round
/// authors an intermediate stop, which is also the only way to say *which*
/// long way.
#[must_use]
pub fn interpolate_in_space(
    space: Colorspace,
    method: InterpMethod,
    from: [f32; 3],
    to: [f32; 3],
    t: f32,
) -> [f32; 3] {
    let t = match method {
        // A step gradient never leaves the stop it is on; the segment's
        // start IS the sample (`color.md` §6).
        InterpMethod::Step => return from,
        InterpMethod::Linear => clamp_unit(t),
        InterpMethod::Smooth => smoothstep(clamp_unit(t)),
    };
    match hue_lane(space) {
        Some(lane) => {
            let mut out = [0.0; 3];
            for index in 0..3 {
                out[index] = if index == lane {
                    lerp_hue_degrees(from[index], to[index], t)
                } else {
                    lerp(from[index], to[index], t)
                };
            }
            out
        }
        None => [
            lerp(from[0], to[0], t),
            lerp(from[1], to[1], t),
            lerp(from[2], to[2], t),
        ],
    }
}

/// Which coordinate of `space` is a hue angle, if any.
fn hue_lane(space: Colorspace) -> Option<usize> {
    match space {
        Colorspace::Hsl | Colorspace::Hsv => Some(0),
        Colorspace::Oklch => Some(2),
        Colorspace::LinearSrgb | Colorspace::Srgb | Colorspace::Oklab => None,
    }
}

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

/// Interpolate two hue angles along the shorter arc, in degrees.
///
/// The result is normalized to `[0,360)`; every consumer of a hue here
/// re-normalizes anyway, so leaving a wrapped-past-the-end value would only
/// make the intermediate values harder to read in a test.
fn lerp_hue_degrees(from: f32, to: f32, t: f32) -> f32 {
    let from = wrap_degrees(from);
    let to = wrap_degrees(to);
    let mut delta = to - from;
    if delta > 180.0 {
        delta -= 360.0;
    } else if delta < -180.0 {
        delta += 360.0;
    }
    wrap_degrees(from + delta * t)
}

/// Fold an angle into `[0,360)`. Manual arithmetic — `f32::rem_euclid` is
/// `std`, and a non-finite hue must not escape as `NaN` into a texel.
fn wrap_degrees(value: f32) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    let mut wrapped = value - ((value / 360.0) as i64) as f32 * 360.0;
    if wrapped < 0.0 {
        wrapped += 360.0;
    }
    if wrapped >= 360.0 {
        wrapped = 0.0;
    }
    wrapped
}

fn clamp_unit(value: f32) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    value.clamp(0.0, 1.0)
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn srgb_triple_to_linear(c: [f32; 3]) -> [f32; 3] {
    [
        srgb_to_linear(c[0]),
        srgb_to_linear(c[1]),
        srgb_to_linear(c[2]),
    ]
}

/// The IEC 61966-2-1 sRGB electro-optical transfer function.
///
/// Applied around the sign so a negative (out-of-gamut) coordinate stays
/// negative instead of becoming a large positive value through the power —
/// the mirrored form every color library uses for extended-range sRGB.
fn srgb_to_linear(value: f32) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    let magnitude = if value < 0.0 { -value } else { value };
    let linear = if magnitude <= 0.040_45 {
        magnitude / 12.92
    } else {
        libm::powf((magnitude + 0.055) / 1.055, 2.4)
    };
    if value < 0.0 { -linear } else { linear }
}

/// HSL (hue degrees, S/L fractions) → display-encoded sRGB.
fn hsl_to_srgb(c: [f32; 3]) -> [f32; 3] {
    let lightness = c[2];
    let saturation = c[1].max(0.0);
    let chroma = (1.0 - abs(2.0 * lightness - 1.0)) * saturation;
    hue_sector(c[0], chroma, lightness - chroma * 0.5)
}

/// HSV/HSB (hue degrees, S/V fractions) → display-encoded sRGB.
fn hsv_to_srgb(c: [f32; 3]) -> [f32; 3] {
    let value = c[2];
    let saturation = c[1].max(0.0);
    let chroma = value * saturation;
    hue_sector(c[0], chroma, value - chroma)
}

/// The shared HSL/HSV sector body: place `chroma` on the hue wheel, then lift
/// the whole triple by `base`.
fn hue_sector(hue_degrees: f32, chroma: f32, base: f32) -> [f32; 3] {
    let sector = wrap_degrees(hue_degrees) / 60.0;
    // `sector % 2` without `f32::rem_euclid`: sector is already in [0,6).
    let sector_pair = sector - (((sector as i32) / 2) * 2) as f32;
    let secondary = chroma * (1.0 - abs(sector_pair - 1.0));
    let rgb = match sector as i32 {
        0 => [chroma, secondary, 0.0],
        1 => [secondary, chroma, 0.0],
        2 => [0.0, chroma, secondary],
        3 => [0.0, secondary, chroma],
        4 => [secondary, 0.0, chroma],
        // `wrap_degrees` bounds the sector to [0,6), so this is the 5 case.
        _ => [chroma, 0.0, secondary],
    };
    [rgb[0] + base, rgb[1] + base, rgb[2] + base]
}

/// Oklch (L, C, hue degrees) → Oklab.
fn oklch_to_oklab(c: [f32; 3]) -> [f32; 3] {
    let radians = wrap_degrees(c[2]) * (core::f32::consts::PI / 180.0);
    [c[0], c[1] * libm::cosf(radians), c[1] * libm::sinf(radians)]
}

/// Oklab → linear sRGB.
///
/// The LMS matrices are the published constants of Björn Ottosson's Oklab
/// definition (`bottosson.github.io/posts/oklab`, 2020), reproduced as
/// numeric transform data rather than adapted code. Values are not clamped —
/// Oklab describes colors outside the sRGB gamut and saying so is the point
/// of using it.
fn oklab_to_linear_srgb(lab: [f32; 3]) -> [f32; 3] {
    let l_ = lab[0] + 0.396_337_78 * lab[1] + 0.215_803_76 * lab[2];
    let m_ = lab[0] - 0.105_561_35 * lab[1] - 0.063_854_17 * lab[2];
    let s_ = lab[0] - 0.089_484_18 * lab[1] - 1.291_485_5 * lab[2];

    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    [
        4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
        -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s,
        -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
    ]
}

fn abs(value: f32) -> f32 {
    if value < 0.0 { -value } else { value }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: [f32; 3], expected: [f32; 3], tolerance: f32) {
        for lane in 0..3 {
            assert!(
                (actual[lane] - expected[lane]).abs() <= tolerance,
                "lane {lane}: {actual:?} != {expected:?}"
            );
        }
    }

    #[test]
    fn linear_srgb_is_the_identity_conversion() {
        let c = [0.25, 0.5, 0.75];
        assert_eq!(to_linear_srgb(Colorspace::LinearSrgb, c), c);
    }

    #[test]
    fn srgb_converts_through_the_standard_transfer_function() {
        // The two anchors every sRGB implementation must agree on, plus the
        // mid-gray whose linear value is the familiar 0.2140.
        close(
            to_linear_srgb(Colorspace::Srgb, [0.0, 1.0, 0.5]),
            [0.0, 1.0, 0.214_041],
            1e-5,
        );
        // Below the knee the curve is the linear segment, not the power.
        close(
            to_linear_srgb(Colorspace::Srgb, [0.03, 0.03, 0.03]),
            [0.03 / 12.92, 0.03 / 12.92, 0.03 / 12.92],
            1e-7,
        );
    }

    #[test]
    fn a_negative_srgb_coordinate_stays_negative() {
        let out = to_linear_srgb(Colorspace::Srgb, [-0.5, 0.0, 0.0]);
        assert!(out[0] < 0.0, "overshoot must not fold to positive: {out:?}");
        assert!((out[0] + 0.214_041).abs() < 1e-5);
    }

    #[test]
    fn hsv_primaries_land_on_the_expected_corners() {
        // Full-saturation, full-value primaries at 0/120/240 degrees.
        close(
            to_linear_srgb(Colorspace::Hsv, [0.0, 1.0, 1.0]),
            [1.0, 0.0, 0.0],
            1e-6,
        );
        close(
            to_linear_srgb(Colorspace::Hsv, [120.0, 1.0, 1.0]),
            [0.0, 1.0, 0.0],
            1e-6,
        );
        close(
            to_linear_srgb(Colorspace::Hsv, [240.0, 1.0, 1.0]),
            [0.0, 0.0, 1.0],
            1e-6,
        );
        // A hue past a full turn is the same color.
        close(
            to_linear_srgb(Colorspace::Hsv, [480.0, 1.0, 1.0]),
            [0.0, 1.0, 0.0],
            1e-6,
        );
    }

    #[test]
    fn hsl_half_lightness_matches_hsv_full_value() {
        close(
            to_linear_srgb(Colorspace::Hsl, [200.0, 1.0, 0.5]),
            to_linear_srgb(Colorspace::Hsv, [200.0, 1.0, 1.0]),
            1e-6,
        );
        // Zero saturation is a neutral at the authored lightness.
        let gray = to_linear_srgb(Colorspace::Hsl, [123.0, 0.0, 0.5]);
        close(gray, [0.214_041, 0.214_041, 0.214_041], 1e-5);
    }

    #[test]
    fn oklab_and_oklch_agree_and_reach_white() {
        // L = 1, no chroma, is white in both parametrizations.
        close(
            to_linear_srgb(Colorspace::Oklab, [1.0, 0.0, 0.0]),
            [1.0, 1.0, 1.0],
            1e-4,
        );
        close(
            to_linear_srgb(Colorspace::Oklch, [1.0, 0.0, 90.0]),
            [1.0, 1.0, 1.0],
            1e-4,
        );
        // Oklch is polar Oklab: hue 0 puts all chroma on `a`.
        close(
            to_linear_srgb(Colorspace::Oklch, [0.6, 0.15, 0.0]),
            to_linear_srgb(Colorspace::Oklab, [0.6, 0.15, 0.0]),
            1e-5,
        );
    }

    #[test]
    fn step_holds_the_segment_start_at_every_position() {
        let from = [1.0, 0.0, 0.0];
        let to = [0.0, 0.0, 1.0];
        for step in 0..=10 {
            let t = step as f32 / 10.0;
            assert_eq!(
                interpolate_in_space(Colorspace::Srgb, InterpMethod::Step, from, to, t),
                from
            );
        }
    }

    #[test]
    fn linear_interpolates_each_lane_of_a_cartesian_space() {
        close(
            interpolate_in_space(
                Colorspace::Srgb,
                InterpMethod::Linear,
                [0.0, 0.0, 0.0],
                [1.0, 0.5, 0.25],
                0.5,
            ),
            [0.5, 0.25, 0.125],
            1e-6,
        );
    }

    #[test]
    fn smooth_matches_linear_at_the_ends_and_eases_between() {
        let from = [0.0, 0.0, 0.0];
        let to = [1.0, 1.0, 1.0];
        close(
            interpolate_in_space(Colorspace::Srgb, InterpMethod::Smooth, from, to, 0.0),
            from,
            1e-6,
        );
        close(
            interpolate_in_space(Colorspace::Srgb, InterpMethod::Smooth, from, to, 1.0),
            to,
            1e-6,
        );
        // Smoothstep is symmetric about the midpoint and slower at 0.25.
        let quarter =
            interpolate_in_space(Colorspace::Srgb, InterpMethod::Smooth, from, to, 0.25)[0];
        assert!(quarter < 0.25, "smoothstep eases in: {quarter}");
        close(
            interpolate_in_space(Colorspace::Srgb, InterpMethod::Smooth, from, to, 0.5),
            [0.5, 0.5, 0.5],
            1e-6,
        );
    }

    #[test]
    fn a_cylindrical_hue_takes_the_short_arc_across_the_wrap() {
        // 350° → 10° is 20° the short way, not 340° the long way.
        let mid = interpolate_in_space(
            Colorspace::Hsv,
            InterpMethod::Linear,
            [350.0, 1.0, 1.0],
            [10.0, 1.0, 1.0],
            0.5,
        );
        assert!(
            (mid[0] - 0.0).abs() < 1e-3 || (mid[0] - 360.0).abs() < 1e-3,
            "expected the wrap midpoint at 0°, got {}",
            mid[0]
        );
        // The Oklch hue lives in lane 2, and its L/C lanes stay linear.
        let oklch = interpolate_in_space(
            Colorspace::Oklch,
            InterpMethod::Linear,
            [0.4, 0.1, 350.0],
            [0.8, 0.2, 10.0],
            0.5,
        );
        assert!((oklch[0] - 0.6).abs() < 1e-6);
        assert!((oklch[1] - 0.15).abs() < 1e-6);
        assert!(
            (oklch[2] - 0.0).abs() < 1e-3 || (oklch[2] - 360.0).abs() < 1e-3,
            "hue lane: {}",
            oklch[2]
        );
    }

    #[test]
    fn a_cartesian_space_never_treats_a_lane_as_an_angle() {
        // Oklab's `b` is a coordinate, not a hue: -0.3 → 0.3 passes through 0.
        let mid = interpolate_in_space(
            Colorspace::Oklab,
            InterpMethod::Linear,
            [0.5, 0.0, -0.3],
            [0.5, 0.0, 0.3],
            0.5,
        );
        assert!((mid[2]).abs() < 1e-6, "{mid:?}");
    }

    #[test]
    fn non_finite_inputs_do_not_escape_as_nan() {
        assert_eq!(wrap_degrees(f32::NAN), 0.0);
        assert_eq!(srgb_to_linear(f32::INFINITY), 0.0);
        let out = interpolate_in_space(
            Colorspace::Srgb,
            InterpMethod::Linear,
            [0.25, 0.25, 0.25],
            [1.0, 1.0, 1.0],
            f32::NAN,
        );
        close(out, [0.25, 0.25, 0.25], 1e-6);
    }
}
