//! Current limiting for a fixture's lamps.
//!
//! A fixture that declares a supply budget has its output scaled down whenever
//! the estimated draw would exceed it. Never up — see [`next_scale_q16`].
//!
//! # Where this sits in the value chain
//!
//! `gamma -> brightness -> POWER SCALE -> colour order`
//!
//! The scale lands **after** gamma, and that ordering is load-bearing. Gamma is
//! nonlinear: scaling its *input* by `s` changes the emitted duty by roughly
//! `s^2.2`, so a scale meant to shed 20% would shed nearly half. Applied after
//! gamma, emitted duty is linear in `s`, which is what lets a scale derived
//! from the budget actually land on it. Brightness — itself a linear
//! post-gamma scale for the same physical reason — lands *before* the power
//! pass, so the accumulated demand is the duty the frame actually asks to
//! emit.
//!
//! # Demand, not emission
//!
//! [`PowerPass::channel`] accumulates the value it is given — post-gamma and
//! **pre-scale** — then returns the scaled one. Accumulating what was actually
//! emitted would close a feedback loop: scale down, the sum falls, the scale
//! rises, the fixture brightens, repeat. The visible result is a fixture
//! pumping once per frame, which looks like a slew-rate problem and is not one.
//! Demand is a function of content alone, so the scale converges.
//!
//! # One frame of latency
//!
//! Demand for a frame is only known once that frame is computed, so a frame's
//! sum sets the *next* frame's scale. The trade-off is real: a single frame can
//! exceed budget. That is fine for a supply with meaningful capacitance and not
//! fine as protection for a hard current limit.

use lpc_model::nodes::fixture::PowerModel;

/// Fixed-point unity. Scales are `0..=UNITY_SCALE_Q16`.
pub(crate) const UNITY_SCALE_Q16: u32 = 1 << 16;

/// How fast the scale is allowed to climb back toward unity, in Q16 per second.
///
/// Roughly two seconds from fully limited to unlimited. Fast enough that a
/// fixture does not stay dim long after a bright moment passes, slow enough
/// that animated content does not make the whole fixture pump as the scale
/// chases it frame to frame. Falling is not rate-limited at all.
const RELEASE_Q16_PER_SECOND: u32 = UNITY_SCALE_Q16 / 2;

/// Per-frame demand accumulator and the scale the frame renders with.
///
/// Construct once per render, hand to the channel writers, then feed to
/// [`estimate_ma`].
pub(crate) struct PowerPass {
    enabled: bool,
    scale_q16: u32,
    /// Sum of post-gamma, pre-scale channel duties in 8-bit units.
    ///
    /// 8 bits is ample for a current estimate and keeps this a single 32-bit
    /// add per channel. Worst case at dome scale — 30k lamps, 3 channels, all
    /// full — is about 23M, well inside `u32`.
    demand8: u32,
}

impl PowerPass {
    /// A pass that limits, applying `scale_q16` and accumulating demand.
    pub(crate) fn limited(scale_q16: u32) -> Self {
        Self {
            enabled: true,
            scale_q16: scale_q16.min(UNITY_SCALE_Q16),
            demand8: 0,
        }
    }

    /// A pass for a fixture with no budget: no scaling, no accumulation.
    pub(crate) fn unlimited() -> Self {
        Self {
            enabled: false,
            scale_q16: UNITY_SCALE_Q16,
            demand8: 0,
        }
    }

    /// Record one channel's demand and return its limited value.
    ///
    /// Call with the post-gamma, post-brightness value, immediately before
    /// writing it out.
    #[inline]
    pub(crate) fn channel(&mut self, value: u16) -> u16 {
        if !self.enabled {
            return value;
        }
        self.demand8 = self.demand8.saturating_add(u32::from(value >> 8));
        if self.scale_q16 >= UNITY_SCALE_Q16 {
            return value;
        }
        ((u32::from(value) * self.scale_q16) >> 16) as u16
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn demand8(&self) -> u32 {
        self.demand8
    }
}

/// An estimate split into the part scaling can shed and the part it cannot.
///
/// The split matters: current limiting only moves the duty term. Treating the
/// total as scalable makes the limiter undershoot its own budget, because it
/// then expects the fixed floor to shrink along with the light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct PowerEstimate {
    /// Quiescent draw. Present whether or not anything is lit.
    pub(crate) idle_ma: u32,
    /// Draw attributable to lit output, at the demand that produced it.
    pub(crate) duty_ma: u32,
}

impl PowerEstimate {
    pub(crate) fn total_ma(self) -> u32 {
        self.idle_ma.saturating_add(self.duty_ma)
    }
}

/// Estimated draw for accumulated `demand8` over `lamp_count` addressable lamps.
///
/// A lamp is one addressable unit. For [`PowerModel::SeriesGroup`] parts one
/// lamp is several physical LEDs sharing a driver channel, so the quiescent
/// term counts every LED while the duty term counts the channel once.
pub(crate) fn estimate_ma(model: PowerModel, lamp_count: u32, demand8: u32) -> PowerEstimate {
    let (per_channel_full, idle_per_led, leds_per_lamp) = match model {
        PowerModel::LinearPerChannel {
            ma_per_channel_full,
            ma_idle_per_led,
        } => (ma_per_channel_full, ma_idle_per_led, 1),
        PowerModel::SeriesGroup {
            ma_per_channel_full,
            ma_idle_per_led,
            leds_per_group,
        } => (ma_per_channel_full, ma_idle_per_led, leds_per_group.max(1)),
    };

    let idle = u64::from(idle_per_led)
        .saturating_mul(u64::from(lamp_count))
        .saturating_mul(u64::from(leds_per_lamp));
    let duty = u64::from(per_channel_full).saturating_mul(u64::from(demand8)) / 255;

    PowerEstimate {
        idle_ma: u32::try_from(idle).unwrap_or(u32::MAX),
        duty_ma: u32::try_from(duty).unwrap_or(u32::MAX),
    }
}

/// The scale the next frame should render with.
///
/// Pure: everything it needs is an argument, and it reads no ambient state.
/// That is deliberate — a later caller composing an additional outer ceiling
/// (a device-level safe clamp, say) can wrap this without restructuring it.
///
/// Only ever returns at most [`UNITY_SCALE_Q16`]. A limiter that could boost
/// would be a different and far more dangerous feature.
pub(crate) fn next_scale_q16(
    estimate: PowerEstimate,
    budget_ma: u32,
    prev_scale_q16: u32,
    dt_ms: u32,
) -> u32 {
    let target = if estimate.total_ma() <= budget_ma {
        UNITY_SCALE_Q16
    } else {
        // Only the duty term responds to scaling, so the budget available to it
        // is what remains after the quiescent floor. Dividing the whole budget
        // by the whole estimate would leave the floor counted twice and settle
        // above budget.
        let headroom = budget_ma.saturating_sub(estimate.idle_ma);
        if headroom == 0 || estimate.duty_ma == 0 {
            // The lamps draw more than the budget even fully dark. Shedding all
            // the light is the most this can do; the supply is undersized.
            0
        } else {
            let scaled = (u64::from(headroom) << 16) / u64::from(estimate.duty_ma);
            u32::try_from(scaled)
                .unwrap_or(UNITY_SCALE_Q16)
                .min(UNITY_SCALE_Q16)
        }
    };

    // Over budget: shed immediately. Waiting out a slew here would keep the
    // supply overloaded for exactly as long as the slew takes.
    if target <= prev_scale_q16 {
        return target;
    }

    let step =
        u32::try_from(u64::from(RELEASE_Q16_PER_SECOND).saturating_mul(u64::from(dt_ms)) / 1000)
            .unwrap_or(UNITY_SCALE_Q16);
    target.min(prev_scale_q16.saturating_add(step))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINEAR: PowerModel = PowerModel::LinearPerChannel {
        ma_per_channel_full: 20,
        ma_idle_per_led: 1,
    };

    #[test]
    fn unlimited_pass_neither_scales_nor_accumulates() {
        let mut pass = PowerPass::unlimited();
        assert_eq!(pass.channel(u16::MAX), u16::MAX);
        assert_eq!(pass.demand8(), 0, "a budget-less fixture pays no cost");
    }

    #[test]
    fn channel_accumulates_before_scaling() {
        let mut pass = PowerPass::limited(UNITY_SCALE_Q16 / 2);
        let out = pass.channel(u16::MAX);
        assert_eq!(out, u16::MAX / 2, "value is halved");
        assert_eq!(
            pass.demand8(),
            255,
            "demand records what was asked for, not what was emitted"
        );
    }

    /// An estimate with no fixed floor, for tests about scaling alone.
    fn duty_only(duty_ma: u32) -> PowerEstimate {
        PowerEstimate {
            idle_ma: 0,
            duty_ma,
        }
    }

    #[test]
    fn estimate_counts_idle_for_every_led_in_a_series_group() {
        let series = PowerModel::SeriesGroup {
            ma_per_channel_full: 20,
            ma_idle_per_led: 1,
            leds_per_group: 3,
        };
        // All dark: only the quiescent term, once per physical LED.
        assert_eq!(estimate_ma(series, 10, 0).total_ma(), 30);
        assert_eq!(estimate_ma(LINEAR, 10, 0).total_ma(), 10);
    }

    #[test]
    fn estimate_scales_with_duty() {
        // 100 lamps, all three channels at full: 300 channels * 255.
        let full = estimate_ma(LINEAR, 100, 300 * 255);
        assert_eq!(full.idle_ma, 100);
        assert_eq!(full.duty_ma, 20 * 300);
        let half = estimate_ma(LINEAR, 100, 300 * 128);
        assert!(half.duty_ma < full.duty_ma);
        assert_eq!(half.idle_ma, full.idle_ma, "the floor does not move");
    }

    #[test]
    fn under_budget_is_unity() {
        assert_eq!(
            next_scale_q16(duty_only(500), 1000, UNITY_SCALE_Q16, 16),
            UNITY_SCALE_Q16
        );
    }

    #[test]
    fn never_exceeds_unity_even_far_under_budget() {
        assert_eq!(
            next_scale_q16(duty_only(1), 100_000, UNITY_SCALE_Q16, 16),
            UNITY_SCALE_Q16
        );
        assert_eq!(
            next_scale_q16(PowerEstimate::default(), 0, UNITY_SCALE_Q16, 16),
            UNITY_SCALE_Q16
        );
    }

    #[test]
    fn over_budget_sheds_in_one_frame() {
        // Twice the budget: halve, immediately, regardless of dt.
        let scale = next_scale_q16(duty_only(2000), 1000, UNITY_SCALE_Q16, 1);
        assert_eq!(scale, UNITY_SCALE_Q16 / 2);
    }

    /// Scaling only moves the duty term, so the fixed floor has to come out of
    /// the budget first. Getting this wrong settles above budget forever.
    #[test]
    fn the_quiescent_floor_comes_out_of_the_budget_first() {
        let estimate = PowerEstimate {
            idle_ma: 100,
            duty_ma: 6000,
        };
        let scale = next_scale_q16(estimate, 1000, UNITY_SCALE_Q16, 1);
        let settled = estimate.idle_ma + (estimate.duty_ma * scale) / UNITY_SCALE_Q16;
        assert!(
            settled <= 1000,
            "settled at {settled} against a 1000 budget"
        );
    }

    #[test]
    fn a_floor_over_budget_sheds_everything() {
        let estimate = PowerEstimate {
            idle_ma: 1500,
            duty_ma: 500,
        };
        assert_eq!(
            next_scale_q16(estimate, 1000, UNITY_SCALE_Q16, 1),
            0,
            "an undersized supply gets all the light we can shed"
        );
    }

    #[test]
    fn release_is_rate_limited() {
        let from = UNITY_SCALE_Q16 / 4;
        let one_frame = next_scale_q16(PowerEstimate::default(), 1000, from, 16);
        assert!(one_frame > from, "it does climb");
        assert!(
            one_frame < UNITY_SCALE_Q16,
            "but not all the way in one frame"
        );

        let long_gap = next_scale_q16(PowerEstimate::default(), 1000, from, 10_000);
        assert_eq!(long_gap, UNITY_SCALE_Q16, "a long gap reaches unity");
    }

    /// The regression test for accumulating emission instead of demand. With
    /// that bug the scale oscillates instead of settling, because each frame's
    /// sum reflects the previous frame's scaling.
    #[test]
    fn constant_content_converges_and_stays_put() {
        let lamp_count = 100;
        let budget_ma = 1000;
        let mut scale = UNITY_SCALE_Q16;
        let mut seen = alloc::vec::Vec::new();

        for _ in 0..200 {
            // Constant full-white demand, measured pre-scale every frame.
            let mut pass = PowerPass::limited(scale);
            for _ in 0..(lamp_count * 3) {
                pass.channel(u16::MAX);
            }
            let estimate = estimate_ma(LINEAR, lamp_count, pass.demand8());
            scale = next_scale_q16(estimate, budget_ma, scale, 16);
            seen.push(scale);
        }

        let settled = seen[seen.len() - 20..].to_vec();
        assert!(
            settled.iter().all(|value| *value == settled[0]),
            "scale must settle, not oscillate: {settled:?}"
        );

        // And it settles somewhere that actually respects the budget.
        let mut pass = PowerPass::limited(settled[0]);
        for _ in 0..(lamp_count * 3) {
            pass.channel(u16::MAX);
        }
        let emitted = estimate_ma(
            LINEAR,
            lamp_count,
            pass.demand8() * settled[0] / UNITY_SCALE_Q16,
        )
        .total_ma();
        assert!(
            emitted <= budget_ma,
            "settled draw {emitted} exceeds budget {budget_ma}"
        );
    }
}
