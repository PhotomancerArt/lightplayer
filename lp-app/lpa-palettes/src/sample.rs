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

/// Inverse of [`to_display_srgb`]: the coordinates an authoring space needs
/// to *show* `srgb`.
///
/// The editor's color well speaks display sRGB (that is what
/// `<input type="color">` is), while a stop's `c` is in the gradient's own
/// [`Colorspace`] — this is the one crossing back. Out-of-gamut coordinates
/// do not survive the round trip (the forward direction clamps into the
/// display cube), which is exactly why an editor converts a color the user
/// just *saw* rather than one it read out of storage.
#[must_use]
pub fn from_display_srgb(space: Colorspace, srgb: [f32; 3]) -> [f32; 3] {
    match space {
        Colorspace::Srgb => srgb,
        Colorspace::LinearSrgb => srgb.map(srgb_to_linear),
        Colorspace::Oklab => display_srgb_to_oklab(srgb),
        // Mirrors `to_display_srgb`'s fallback for the spaces the catalog
        // does not use: a clamp, never a panic.
        Colorspace::Hsl | Colorspace::Hsv | Colorspace::Oklch => [
            srgb[0].clamp(0.0, 1.0),
            srgb[1].clamp(0.0, 1.0),
            srgb[2].clamp(0.0, 1.0),
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

fn srgb_to_linear(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
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

/// Inverse of [`oklab_to_display_srgb`] — the standard linear-sRGB → LMS →
/// cube-root → Oklab chain.
fn display_srgb_to_oklab(srgb: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = srgb.map(srgb_to_linear);

    let l = 0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b;
    let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
    let s = 0.088_302_46 * r + 0.281_718_84 * g + 0.629_978_5 * b;

    let l_ = l.cbrt();
    let m_ = m.cbrt();
    let s_ = s.cbrt();

    [
        0.210_454_26 * l_ + 0.793_617_8 * m_ - 0.004_072_047 * s_,
        1.977_998_5 * l_ - 2.428_592_2 * m_ + 0.450_593_7 * s_,
        0.025_904_037 * l_ + 0.782_771_77 * m_ - 0.808_675_77 * s_,
    ]
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

    /// The editor's color well hands back display sRGB; the stop it lands in
    /// is in the gradient's own space, so the crossing has to survive a round
    /// trip for in-gamut colors.
    #[test]
    fn display_srgb_round_trips_through_every_authoring_space_the_editor_offers() {
        for space in [Colorspace::Srgb, Colorspace::LinearSrgb, Colorspace::Oklab] {
            for srgb in [
                [0.0, 0.0, 0.0],
                [1.0, 1.0, 1.0],
                [0.2, 0.55, 0.9],
                [0.93, 0.41, 0.06],
            ] {
                let coords = from_display_srgb(space, srgb);
                let back = to_display_srgb(space, coords);
                for channel in 0..3 {
                    assert!(
                        (back[channel] - srgb[channel]).abs() < 2e-3,
                        "{space:?}: {srgb:?} -> {coords:?} -> {back:?}"
                    );
                }
            }
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
