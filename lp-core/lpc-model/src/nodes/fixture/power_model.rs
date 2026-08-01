//! How a lamp's current draw relates to its PWM duty.
//!
//! A lamp type carries a *model kind*, not a single milliamp number, because
//! 5V and 12V parts do not scale the same way.
//!
//! - 5V WS2812-family parts drive each colour die directly, so draw is close to
//!   proportional to per-channel duty, plus a small per-LED quiescent term.
//! - 12V parts differ structurally. Some (WS2811 strips) drive several LEDs in
//!   **series** from one driver channel, so the channel's current covers the
//!   whole group rather than each LED. Others run constant-current drivers at a
//!   much lower per-channel current than their 5V cousins.
//!
//! The quiescent term matters more than it looks: it is independent of colour,
//! so it dominates at low brightness — exactly where installations run. A
//! duty-only formula omits it entirely and under-estimates draw.
//!
//! Evaluating these models is the limiter's job; this module only defines their
//! shape. The numbers live in [`super::lamp_presets`].

use serde::{Deserialize, Serialize};

/// The current-draw model for a lamp type.
///
/// All quantities are milliamps at the lamp's own supply voltage. Comparing
/// figures across voltages is meaningless — a 12V part drawing 6 mA and a 5V
/// part drawing 20 mA can consume similar power.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum PowerModel {
    /// One driver channel per colour die. Draw scales with per-channel duty,
    /// plus a fixed per-LED quiescent term.
    ///
    /// Typical of 5V WS2812-family parts, and of 12V per-pixel parts whose
    /// constant-current drivers simply run at a lower per-channel current.
    LinearPerChannel {
        /// Milliamps drawn by one colour channel at full duty.
        ma_per_channel_full: u32,
        /// Milliamps drawn per LED regardless of output.
        ma_idle_per_led: u32,
    },
    /// One driver channel feeds several LEDs wired in **series**. The channel's
    /// current covers the whole group, so adding LEDs within a group costs
    /// voltage headroom rather than current.
    ///
    /// Typical of 12V WS2811 strips, where three LEDs share one addressable
    /// chip.
    SeriesGroup {
        /// Milliamps drawn by one colour channel at full duty, for the whole
        /// series group.
        ma_per_channel_full: u32,
        /// Milliamps drawn per LED regardless of output.
        ma_idle_per_led: u32,
        /// How many physical LEDs one driver channel feeds in series.
        leds_per_group: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_round_trip_as_tagged_json() {
        let linear = PowerModel::LinearPerChannel {
            ma_per_channel_full: 20,
            ma_idle_per_led: 1,
        };
        let json = serde_json::to_string(&linear).expect("encodes");
        assert!(
            json.contains("linear_per_channel"),
            "snake_case tag: {json}"
        );
        assert_eq!(
            serde_json::from_str::<PowerModel>(&json).expect("decodes"),
            linear
        );
    }

    #[test]
    fn series_group_carries_its_group_size() {
        let series = PowerModel::SeriesGroup {
            ma_per_channel_full: 20,
            ma_idle_per_led: 1,
            leds_per_group: 3,
        };
        let PowerModel::SeriesGroup { leds_per_group, .. } = series else {
            panic!("expected series group");
        };
        assert_eq!(leds_per_group, 3);
    }
}
