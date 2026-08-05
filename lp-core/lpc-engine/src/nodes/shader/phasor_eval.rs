//! Shaping a phasor's raw ramp into the value a `float` uniform receives.
//!
//! The timebase store's contract is the **raw** wrapped `[0,1)` ramp (parent
//! D8) — waveform and phase offset are applied here, on the way out, so that
//! two consumers of one shared integrator can read the same cycle with
//! different shaping.
//!
//! Every waveform outputs `[0,1]`: a shader author binding `uniform float
//! wave;` should never have to ask which half of the unit interval a
//! particular waveform lives in.

use lpc_model::{PhasorConfig, Waveform};

/// Apply `config`'s phase offset and waveform to a raw ramp position.
///
/// `phase` is the store's wrapped ramp; the offset is added and re-wrapped
/// before shaping, which is what makes `phase_offset` a *rotation* of the
/// cycle rather than a bias on the output.
#[must_use]
pub fn shape_phasor(config: &PhasorConfig, phase: f32) -> f32 {
    let x = wrap_unit(phase + config.phase_offset);
    match config.waveform {
        Waveform::Ramp => x,
        // libm, not `f32::sin`: this path compiles for every firmware tier
        // and `core` has no float transcendentals.
        Waveform::Sine => 0.5 + 0.5 * libm::sinf(core::f32::consts::TAU * x),
        Waveform::Triangle => {
            if x < 0.5 {
                2.0 * x
            } else {
                2.0 - 2.0 * x
            }
        }
        Waveform::Square => {
            if x < 0.5 {
                0.0
            } else {
                1.0
            }
        }
    }
}

/// The value a phasor uniform holds before any timebase has advanced it —
/// frame 0, and the fallback whenever no time product resolves.
#[must_use]
pub fn phasor_frame_zero(config: &PhasorConfig) -> f32 {
    shape_phasor(config, 0.0)
}

/// Fold `value` into `[0,1)`. Manual arithmetic: `f32::floor` is `std`, and
/// this runs on every tier.
fn wrap_unit(value: f32) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    let mut frac = value - (value as i64) as f32;
    if frac < 0.0 {
        frac += 1.0;
    }
    // A tiny negative fraction can round to exactly 1.0 once 1.0 is added;
    // `[0,1)` is a promise, so close the interval by hand.
    if frac >= 1.0 {
        frac = 0.0;
    }
    frac
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(waveform: Waveform, phase_offset: f32) -> PhasorConfig {
        PhasorConfig {
            period_seconds: 1.0,
            waveform,
            phase_offset,
        }
    }

    #[test]
    fn ramp_passes_the_raw_phase_through() {
        let ramp = config(Waveform::Ramp, 0.0);

        assert_eq!(shape_phasor(&ramp, 0.0), 0.0);
        assert_eq!(shape_phasor(&ramp, 0.25), 0.25);
        assert!((shape_phasor(&ramp, 0.999) - 0.999).abs() < 1e-6);
    }

    #[test]
    fn sine_spans_the_unit_interval_starting_at_its_midpoint() {
        let sine = config(Waveform::Sine, 0.0);

        assert!((shape_phasor(&sine, 0.0) - 0.5).abs() < 1e-6);
        assert!(
            (shape_phasor(&sine, 0.25) - 1.0).abs() < 1e-6,
            "sine peaks at a quarter cycle: {}",
            shape_phasor(&sine, 0.25)
        );
        assert!((shape_phasor(&sine, 0.5) - 0.5).abs() < 1e-6);
        assert!((shape_phasor(&sine, 0.75) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn triangle_and_square_stay_in_the_unit_interval() {
        let triangle = config(Waveform::Triangle, 0.0);
        let square = config(Waveform::Square, 0.0);

        assert_eq!(shape_phasor(&triangle, 0.0), 0.0);
        assert_eq!(shape_phasor(&triangle, 0.25), 0.5);
        assert_eq!(shape_phasor(&triangle, 0.5), 1.0);
        assert_eq!(shape_phasor(&triangle, 0.75), 0.5);

        assert_eq!(shape_phasor(&square, 0.0), 0.0);
        assert_eq!(shape_phasor(&square, 0.49), 0.0);
        assert_eq!(shape_phasor(&square, 0.5), 1.0);
        assert_eq!(shape_phasor(&square, 0.99), 1.0);
    }

    #[test]
    fn every_waveform_answers_inside_the_unit_interval() {
        for &waveform in Waveform::all() {
            let config = config(waveform, 0.125);
            for step in 0..=64 {
                let phase = step as f32 / 64.0;
                let value = shape_phasor(&config, phase);
                assert!(
                    (0.0..=1.0).contains(&value),
                    "{waveform} escaped [0,1] at {phase}: {value}"
                );
            }
        }
    }

    #[test]
    fn the_offset_rotates_the_cycle_and_re_wraps() {
        let offset = config(Waveform::Ramp, 0.25);

        assert_eq!(shape_phasor(&offset, 0.0), 0.25);
        assert!((shape_phasor(&offset, 0.9) - 0.15).abs() < 1e-6);
        // A negative offset wraps up rather than escaping the interval.
        assert!((shape_phasor(&config(Waveform::Ramp, -0.25), 0.1) - 0.85).abs() < 1e-6);
    }

    #[test]
    fn frame_zero_is_the_offset_under_the_authored_waveform() {
        assert_eq!(phasor_frame_zero(&config(Waveform::Ramp, 0.25)), 0.25);
        assert_eq!(phasor_frame_zero(&config(Waveform::Square, 0.75)), 1.0);
        assert!((phasor_frame_zero(&config(Waveform::Sine, 0.25)) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_non_finite_phase_does_not_escape_the_interval() {
        let ramp = config(Waveform::Ramp, 0.0);

        assert_eq!(shape_phasor(&ramp, f32::NAN), 0.0);
        assert_eq!(shape_phasor(&ramp, f32::INFINITY), 0.0);
    }
}
