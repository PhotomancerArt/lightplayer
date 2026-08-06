//! Turning a [`GradientConfig`] and one phasor position into the bake a
//! palette uniform shows this tick.
//!
//! The counterpart of [`phasor_eval`](super::phasor_eval): the timebase store
//! answers the raw wrapped ramp, and everything about *where in the cycle*
//! that puts the palette is decided here, as a pure function of φ. Nothing in
//! this module reads a clock, keeps state, or allocates.
//!
//! # One phasor, not one per entry
//!
//! A cycle's period is `set.len() × step_seconds`
//! ([`GradientConfig::full_cycle_seconds`]), so a single φ carries both the
//! entry index (`floor(φ·N)`) and the position inside that entry. That is
//! what makes scrubbing exact: the same effective time reproduces the same φ
//! through the store's closed-form evaluation, and the same φ reproduces the
//! same index and mix through this module, bit for bit.
//!
//! Asking for one phasor per entry would have made the palette's phase a
//! function of *which* entries had run, which is state, and state is what
//! `GradientConfig` deliberately is not.
//!
//! # Frozen
//!
//! `step_seconds <= 0` or non-finite is frozen
//! ([`GradientConfig::is_frozen`]). That is not special-cased here: a frozen
//! cycle's full-cycle period is `0.0`, [`PhasorConfig::rate_hz`] turns a
//! non-positive period into rate 0, and the store then holds the phase
//! wherever it already was. Dragging the step to zero holds the palette on
//! the entry it is on; it does not snap back to the first.

use lpc_model::{Gradient, GradientConfig, PhasorConfig, Waveform};

/// Quantization steps for a cross-fade's mix.
///
/// A fade's mix moves continuously, and every distinct value is a distinct
/// bake — so an unquantized fade re-bakes the strip on every frame it is
/// visible. Snapping to 64ths bounds a fade to 64 bakes however long it lasts
/// (a 4-second fade at 60 fps goes from 240 to 64) and lets consecutive
/// frames share one cached texture. 64 steps on a 16-bit texel is well below
/// what the eye resolves in a dissolve; it is a cache key, not a dither.
pub const PALETTE_MIX_STEPS: u32 = 64;

/// Which gradients a palette shows this tick, and how much of each.
///
/// `mix == 0` means `from` alone — the common case, and the one that lets a
/// non-fading cycle reuse a single static bake per entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaletteCyclePosition {
    /// Index into the config's set of the entry currently held.
    pub from: usize,
    /// Index of the entry being faded toward. Equals `from` when `mix == 0`.
    pub to: usize,
    /// Quantized cross-fade position in `0..=PALETTE_MIX_STEPS`.
    pub mix_steps: u32,
}

impl PaletteCyclePosition {
    /// The cross-fade position as a fraction.
    #[must_use]
    pub fn mix(self) -> f32 {
        self.mix_steps as f32 / PALETTE_MIX_STEPS as f32
    }

    /// Whether this position is one gradient held, needing no blend bake.
    #[must_use]
    pub fn is_single(self) -> bool {
        self.mix_steps == 0 || self.from == self.to
    }
}

/// The phasor config a palette cycle queries the timebase with.
///
/// Deliberately unshaped: `Ramp` with no offset, because this module wants
/// the raw cycle position and any waveform would make "how far through the
/// set are we" a non-monotonic question. The period is the *whole* set's
/// pass, per the module docs.
#[must_use]
pub fn palette_phasor_config(config: &GradientConfig) -> PhasorConfig {
    PhasorConfig {
        period_seconds: config.full_cycle_seconds(),
        waveform: Waveform::Ramp,
        phase_offset: 0.0,
    }
}

/// Where `phase` puts `config` in its cycle.
///
/// `phase` is the store's raw wrapped `[0,1)` ramp. A static config ignores
/// it entirely — there is one gradient and it is always shown.
#[must_use]
pub fn palette_cycle_position(config: &GradientConfig, phase: f32) -> PaletteCyclePosition {
    let count = config.gradients().len();
    if count <= 1 {
        return PaletteCyclePosition {
            from: 0,
            to: 0,
            mix_steps: 0,
        };
    }
    let GradientConfig::Cycle {
        step_seconds,
        fade_seconds,
        ..
    } = config
    else {
        return PaletteCyclePosition {
            from: 0,
            to: 0,
            mix_steps: 0,
        };
    };

    let phase = wrap_unit(phase);
    let position = phase * count as f32;
    // `position` is in `[0, count)` by construction, but a phase of exactly
    // 1.0−ε times a large count can round up; clamping is cheaper than
    // trusting the multiply.
    let from = (position as usize).min(count - 1);
    let within_step = position - from as f32;

    PaletteCyclePosition {
        from,
        to: (from + 1) % count,
        mix_steps: mix_steps(within_step, *step_seconds, *fade_seconds),
    }
}

/// The gradients a position names, borrowed from the config.
///
/// Returns `(from, to)`; the two are the same gradient when the position is
/// single. `None` only for a config with no gradients at all, which
/// [`GradientConfig::validate`] rejects but a hand-built def can reach.
#[must_use]
pub fn palette_cycle_gradients<'a>(
    config: &'a GradientConfig,
    position: PaletteCyclePosition,
) -> Option<(&'a Gradient, &'a Gradient)> {
    let gradients = config.gradients();
    let from = gradients.get(position.from)?;
    let to = gradients.get(position.to).unwrap_or(from);
    Some((from, to))
}

/// The position a palette holds before any timebase has advanced it — frame
/// 0, and the fallback whenever no time product resolves.
///
/// Mirrors [`phasor_frame_zero`](super::phasor_eval::phasor_frame_zero):
/// deterministic, never a panic, and the honest answer is the start of the
/// first cycle.
#[must_use]
pub fn palette_frame_zero(config: &GradientConfig) -> PaletteCyclePosition {
    palette_cycle_position(config, 0.0)
}

/// The quantized cross-fade for a position `within_step` through one entry.
///
/// The fade is carved out of the **tail** of the step it precedes: an entry
/// with a 8 s step and a 2 s fade holds alone for 6 s, then dissolves into
/// its successor over the last 2 s. Carving from the tail rather than
/// straddling the boundary is what keeps `from` the entry you would name if
/// asked "which palette is showing".
fn mix_steps(within_step: f32, step_seconds: f32, fade_seconds: f32) -> u32 {
    if !fade_seconds.is_finite() || fade_seconds <= 0.0 {
        return 0;
    }
    if !step_seconds.is_finite() || step_seconds <= 0.0 {
        // Frozen: the phase is held, and a held phase has no dissolve to be
        // partway through.
        return 0;
    }
    // A fade at least as long as the step is a continuous cross-fade.
    let fade_fraction = (fade_seconds / step_seconds).min(1.0);
    let fade_start = 1.0 - fade_fraction;
    if within_step <= fade_start {
        return 0;
    }
    let raw = (within_step - fade_start) / fade_fraction;
    quantize_mix(raw)
}

/// Snap a `[0,1]` mix onto the [`PALETTE_MIX_STEPS`] grid.
fn quantize_mix(mix: f32) -> u32 {
    if !mix.is_finite() || mix <= 0.0 {
        return 0;
    }
    let steps = (mix * PALETTE_MIX_STEPS as f32 + 0.5) as u32;
    steps.min(PALETTE_MIX_STEPS)
}

/// Fold `value` into `[0,1)`. Same manual arithmetic as the phasor
/// evaluator's, and for the same reason: `f32::floor` is `std`.
fn wrap_unit(value: f32) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    let mut frac = value - (value as i64) as f32;
    if frac < 0.0 {
        frac += 1.0;
    }
    if frac >= 1.0 {
        frac = 0.0;
    }
    frac
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use lpc_model::{Colorspace, GradientStop, InterpMethod};

    use super::*;

    #[test]
    fn a_static_config_is_one_gradient_at_every_phase() {
        let config = GradientConfig::Static(gradient(0.0));

        for step in 0..16 {
            let position = palette_cycle_position(&config, step as f32 / 16.0);
            assert_eq!(position.from, 0);
            assert!(position.is_single());
        }
        assert_eq!(palette_phasor_config(&config).period_seconds, 0.0);
    }

    #[test]
    fn a_cycles_period_is_the_whole_set_and_the_index_is_floor_of_phi_times_n() {
        let config = cycle(4, 3.0, 0.0);
        assert_eq!(palette_phasor_config(&config).period_seconds, 12.0);

        for (phase, expected) in [
            (0.0, 0usize),
            (0.24, 0),
            (0.25, 1),
            (0.51, 2),
            (0.75, 3),
            (0.999, 3),
        ] {
            let position = palette_cycle_position(&config, phase);
            assert_eq!(position.from, expected, "phase {phase}");
            assert!(position.is_single(), "no fade authored");
            assert_eq!(position.to, (expected + 1) % 4);
        }
    }

    #[test]
    fn the_fade_is_carved_out_of_the_tail_of_the_step_it_precedes() {
        // 4 entries, 4 s step, 1 s fade: each entry holds alone for the first
        // three quarters of its step, then dissolves over the last quarter.
        let config = cycle(4, 4.0, 1.0);

        // Inside the first entry's hold.
        let held = palette_cycle_position(&config, 0.10);
        assert_eq!((held.from, held.mix_steps), (0, 0));

        // The instant the fade opens.
        let opening = palette_cycle_position(&config, 0.25 * 0.75);
        assert_eq!(opening.from, 0);
        assert_eq!(opening.mix_steps, 0);

        // Halfway through the fade.
        let midway = palette_cycle_position(&config, 0.25 * 0.875);
        assert_eq!(midway.from, 0);
        assert_eq!(midway.to, 1);
        assert!((midway.mix() - 0.5).abs() < 0.02, "{}", midway.mix());
        assert!(!midway.is_single());

        // The last entry fades into the first — the cycle closes.
        let closing = palette_cycle_position(&config, 0.999);
        assert_eq!((closing.from, closing.to), (3, 0));
        assert!(closing.mix_steps > 0);
    }

    #[test]
    fn a_fade_at_least_as_long_as_the_step_is_a_continuous_cross_fade() {
        let config = cycle(2, 2.0, 8.0);

        // Never held alone: the mix is already moving at the step's start.
        let early = palette_cycle_position(&config, 0.01);
        assert!(early.mix_steps > 0, "{early:?}");
        let late = palette_cycle_position(&config, 0.49);
        assert!(late.mix_steps > early.mix_steps);
    }

    #[test]
    fn the_mix_is_quantized_so_neighbouring_frames_share_a_bake() {
        let config = cycle(2, 4.0, 4.0);
        // Two phases a thousandth apart land on the same quantized mix, which
        // is exactly what makes the bake cache hit during a fade.
        let a = palette_cycle_position(&config, 0.2000);
        let b = palette_cycle_position(&config, 0.2005);
        assert_eq!(a, b);

        for steps in 0..=PALETTE_MIX_STEPS {
            assert!(steps <= PALETTE_MIX_STEPS);
        }
        assert_eq!(quantize_mix(-1.0), 0);
        assert_eq!(quantize_mix(f32::NAN), 0);
        assert_eq!(quantize_mix(2.0), PALETTE_MIX_STEPS);
    }

    #[test]
    fn a_frozen_cycle_asks_for_a_zero_rate_and_never_dissolves() {
        for step in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let config = cycle(3, step, 1.0);
            assert!(config.is_frozen(), "step {step}");
            // Period 0 → `PhasorConfig::rate_hz() == 0` → the store holds the
            // phase where it already was, which is the frozen contract.
            assert_eq!(palette_phasor_config(&config).rate_hz(), 0.0);
            // And wherever it is held, it is held on one entry, not mid-fade.
            let position = palette_cycle_position(&config, 0.6);
            assert_eq!(position.from, 1, "holds the entry phi is on");
            assert!(position.is_single());
        }
    }

    #[test]
    fn frame_zero_is_the_start_of_the_first_entry() {
        assert_eq!(
            palette_frame_zero(&cycle(4, 2.0, 0.5)),
            PaletteCyclePosition {
                from: 0,
                to: 1,
                mix_steps: 0,
            }
        );
        assert!(palette_frame_zero(&GradientConfig::default()).is_single());
    }

    #[test]
    fn the_same_phase_always_resolves_to_the_same_position() {
        let config = cycle(5, 3.0, 1.0);
        // Scrub forward and back over the same phases; every revisit must
        // land on the identical position (index AND quantized mix).
        let phases: alloc::vec::Vec<f32> = (0..200).map(|i| i as f32 / 199.0).collect();
        let forward: alloc::vec::Vec<_> = phases
            .iter()
            .map(|phase| palette_cycle_position(&config, *phase))
            .collect();
        let backward: alloc::vec::Vec<_> = phases
            .iter()
            .rev()
            .map(|phase| palette_cycle_position(&config, *phase))
            .collect();

        for (index, position) in backward.iter().rev().enumerate() {
            assert_eq!(*position, forward[index], "phase {}", phases[index]);
        }
    }

    #[test]
    fn a_non_finite_phase_does_not_escape_the_set() {
        let config = cycle(4, 2.0, 0.5);
        for phase in [f32::NAN, f32::INFINITY, -f32::INFINITY] {
            let position = palette_cycle_position(&config, phase);
            assert!(position.from < 4, "{position:?}");
            assert!(position.to < 4, "{position:?}");
        }
        // A negative phase wraps up rather than indexing backwards.
        assert_eq!(palette_cycle_position(&config, -0.1).from, 3);
    }

    #[test]
    fn the_gradients_a_position_names_come_from_the_configs_set() {
        let config = cycle(3, 2.0, 1.0);
        let position = PaletteCyclePosition {
            from: 2,
            to: 0,
            mix_steps: 32,
        };
        let (from, to) = palette_cycle_gradients(&config, position).expect("gradients");

        assert_eq!(from.stops[0].c[0], 2.0);
        assert_eq!(to.stops[0].c[0], 0.0);
    }

    /// A gradient tagged with `marker` in its first stop's red lane, so a
    /// test can say which set entry it got.
    fn gradient(marker: f32) -> Gradient {
        Gradient {
            space: Colorspace::LinearSrgb,
            method: InterpMethod::Linear,
            stops: vec![
                GradientStop {
                    at: 0.0,
                    c: [marker, 0.0, 0.0],
                },
                GradientStop {
                    at: 1.0,
                    c: [marker, 1.0, 1.0],
                },
            ],
        }
    }

    fn cycle(count: usize, step_seconds: f32, fade_seconds: f32) -> GradientConfig {
        GradientConfig::Cycle {
            set: (0..count).map(|index| gradient(index as f32)).collect(),
            step_seconds,
            fade_seconds,
        }
    }
}
