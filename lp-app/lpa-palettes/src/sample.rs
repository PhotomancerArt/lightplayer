//! Sample a [`Gradient`] into display-ready color.
//!
//! **Contract**: interpolation happens in the gradient's own authored
//! [`Colorspace`] (never in a canonical space first — that is the point of
//! authoring in Oklab/Oklch), and [`to_display_srgb`] is the one place a
//! sampled color crosses into gamma-encoded sRGB for display. Callers that
//! need `u8` pixels round `srgb[i] * 255.0` themselves — this module stays
//! in `f32 [0, 1]` so it composes with any output resolution.
//!
//! Promoted out of the `strip_sheet` example (M3 curation-gate artifact) so
//! the studio-web `GradientStripCanvas` component (M4 P1) can share the same
//! sampling code the contact-sheet renderer already proved correct, instead
//! of a second hand-rolled copy drifting from it.

use lpc_model::{Colorspace, Gradient, GradientStop, InterpMethod};

/// Sample `gradient` at `t` and convert the result to display sRGB.
///
/// `t` is expected in `[0, 1]`; values outside that range clamp to the
/// nearest end stop the same way an in-range `t` beyond the last stop does.
#[must_use]
pub fn sample_gradient_as_srgb(gradient: &Gradient, t: f32) -> [f32; 3] {
    let mut stops = gradient.stops.clone();
    stops.sort_by(|a, b| a.at.total_cmp(&b.at));

    let raw = match gradient.method {
        InterpMethod::Step => sample_step(&stops, t),
        InterpMethod::Linear | InterpMethod::Smooth => sample_linear(&stops, t),
    };

    to_display_srgb(gradient.space, raw)
}

/// No interpolation: the nearest stop at or before `t` (what a discrete
/// swatch palette samples as).
#[must_use]
pub fn sample_step(stops: &[GradientStop], t: f32) -> [f32; 3] {
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

/// Linear interpolation between the two stops bracketing `t`. Also used for
/// [`InterpMethod::Smooth`] here — a preview only needs the color sequence
/// to be legible, not the easing curve.
#[must_use]
pub fn sample_linear(stops: &[GradientStop], t: f32) -> [f32; 3] {
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
/// (gamma-encoded) sRGB. `Srgb` is already display-encoded; `Oklab` goes
/// through the standard Oklab -> linear sRGB matrices, then gamma-encodes.
#[must_use]
pub fn to_display_srgb(space: Colorspace, c: [f32; 3]) -> [f32; 3] {
    match space {
        Colorspace::Srgb => c,
        Colorspace::LinearSrgb => c.map(linear_to_srgb),
        Colorspace::Oklab => oklab_to_display_srgb(c),
        // Hsl/Hsv/Oklch aren't used by the M3 catalog; fall back to a clamp
        // so a sampler never panics if one is added later.
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

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::GradientStop;

    fn two_stop(space: Colorspace, method: InterpMethod) -> Gradient {
        Gradient {
            space,
            method,
            stops: vec![
                GradientStop {
                    at: 0.0,
                    c: [0.0, 0.0, 0.0],
                },
                GradientStop {
                    at: 1.0,
                    c: [1.0, 1.0, 1.0],
                },
            ],
        }
    }

    #[test]
    fn linear_srgb_gradient_samples_the_midpoint_at_half() {
        let gradient = two_stop(Colorspace::Srgb, InterpMethod::Linear);

        assert_eq!(sample_gradient_as_srgb(&gradient, 0.0), [0.0, 0.0, 0.0]);
        assert_eq!(sample_gradient_as_srgb(&gradient, 1.0), [1.0, 1.0, 1.0]);
        let mid = sample_gradient_as_srgb(&gradient, 0.5);
        for channel in mid {
            assert!((channel - 0.5).abs() < 1e-6, "{mid:?}");
        }
    }

    #[test]
    fn step_gradient_holds_the_nearest_prior_stop() {
        let gradient = two_stop(Colorspace::Srgb, InterpMethod::Step);

        assert_eq!(sample_gradient_as_srgb(&gradient, 0.0), [0.0, 0.0, 0.0]);
        assert_eq!(sample_gradient_as_srgb(&gradient, 0.49), [0.0, 0.0, 0.0]);
        assert_eq!(sample_gradient_as_srgb(&gradient, 1.0), [1.0, 1.0, 1.0]);
    }

    #[test]
    fn linear_srgb_space_gamma_encodes_on_the_way_to_display() {
        let gradient = two_stop(Colorspace::LinearSrgb, InterpMethod::Linear);

        // Linear 0.5 is brighter than sRGB-encoded 0.5 (gamma ~2.2 pulls
        // midtones down), so the display value must be higher.
        let displayed = sample_gradient_as_srgb(&gradient, 0.5);
        assert!(displayed[0] > 0.5, "{displayed:?}");
    }

    #[test]
    fn oklab_black_and_white_round_trip_to_display_black_and_white() {
        let gradient = Gradient {
            space: Colorspace::Oklab,
            method: InterpMethod::Linear,
            stops: vec![
                GradientStop {
                    at: 0.0,
                    c: [0.0, 0.0, 0.0],
                },
                GradientStop {
                    at: 1.0,
                    c: [1.0, 0.0, 0.0],
                },
            ],
        };

        let black = sample_gradient_as_srgb(&gradient, 0.0);
        for channel in black {
            assert!(channel.abs() < 1e-4, "{black:?}");
        }
        let white = sample_gradient_as_srgb(&gradient, 1.0);
        for channel in white {
            assert!((channel - 1.0).abs() < 1e-3, "{white:?}");
        }
    }

    #[test]
    fn out_of_gamut_space_clamps_instead_of_panicking() {
        let gradient = two_stop(Colorspace::Hsl, InterpMethod::Linear);
        let sampled = sample_gradient_as_srgb(&gradient, 0.5);
        for channel in sampled {
            assert!((0.0..=1.0).contains(&channel), "{sampled:?}");
        }
    }
}
