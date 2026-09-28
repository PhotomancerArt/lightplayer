//! What the status LED shows, as a pure function of state and time.
//!
//! Each state has a pattern a person can tell apart across a room:
//!
//! | state          | pattern                                              |
//! |----------------|------------------------------------------------------|
//! | `Booting`      | steady on                                            |
//! | `Running`      | on, with a short dark blip every 2 s                 |
//! | `PoweringOff`  | three flashes, then dark — and dark stays through deep sleep |
//!
//! `Running` blinks rather than holding steady on purpose: a steady LED
//! looks the same whether the firmware is running or hung with the pin
//! still driven. The blip needs the firmware's executor to keep scheduling,
//! so a board that is on and not blipping has not started — or has stopped.
//!
//! `Booting` is steady because it has to be: the firmware's boot runs to the
//! server loop without yielding, so nothing could animate it. Steady and
//! never blipping is therefore what a boot that hangs looks like.
//!
//! New states go here, with their pattern in the table and a test.

/// Running: one dark blip of this length at the end of every period.
const RUN_PERIOD_MS: u64 = 2_000;
const RUN_BLIP_MS: u64 = 100;
/// Powering off: dark, lit, dark, lit, dark, lit — then dark for good.
const POWER_OFF_STEP_MS: u64 = 150;
const POWER_OFF_FLASHES: u64 = 3;

/// What LightPlayer is doing, as the status LED shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLedState {
    /// From the first moment the firmware knows its board until the server
    /// loop starts.
    Booting,
    /// The server loop is running.
    Running,
    /// A power-off was accepted; deep sleep follows.
    PoweringOff,
}

impl StatusLedState {
    /// Whether the LED is lit `elapsed_ms` after this state began.
    pub fn lit_at(self, elapsed_ms: u64) -> bool {
        match self {
            Self::Booting => true,
            Self::Running => elapsed_ms % RUN_PERIOD_MS < RUN_PERIOD_MS - RUN_BLIP_MS,
            Self::PoweringOff => {
                let step = elapsed_ms / POWER_OFF_STEP_MS;
                step < 2 * POWER_OFF_FLASHES && step % 2 == 1
            }
        }
    }

    /// How long the state's pattern takes to finish, for a state that
    /// finishes: after it, the LED holds `lit_at`'s last value for good.
    pub fn pattern_ms(self) -> Option<u64> {
        match self {
            Self::Booting | Self::Running => None,
            Self::PoweringOff => Some(2 * POWER_OFF_FLASHES * POWER_OFF_STEP_MS),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// The LED's level every 50 ms over `ms`, as `#` lit and `.` dark.
    fn strip(state: StatusLedState, ms: u64) -> alloc::string::String {
        (0..ms / 50)
            .map(|i| if state.lit_at(i * 50) { '#' } else { '.' })
            .collect()
    }

    #[test]
    fn booting_is_steady_on() {
        assert_eq!(strip(StatusLedState::Booting, 800), "################");
    }

    #[test]
    fn running_is_lit_with_a_dark_blip_every_two_seconds() {
        let strip = strip(StatusLedState::Running, 4_000);
        let dark: Vec<usize> = strip
            .char_indices()
            .filter(|(_, c)| *c == '.')
            .map(|(i, _)| i * 50)
            .collect();

        assert_eq!(dark, [1_900, 1_950, 3_900, 3_950]);
        assert!(
            StatusLedState::Running.lit_at(0),
            "lit the moment it starts"
        );
    }

    #[test]
    fn powering_off_flashes_three_times_then_stays_dark() {
        assert_eq!(
            strip(StatusLedState::PoweringOff, 1_200),
            "...###...###...###......"
        );
        assert_eq!(StatusLedState::PoweringOff.pattern_ms(), Some(900));
        assert!(!StatusLedState::PoweringOff.lit_at(900));
        assert!(!StatusLedState::PoweringOff.lit_at(u64::MAX));
    }

    #[test]
    fn only_powering_off_finishes() {
        assert_eq!(StatusLedState::Booting.pattern_ms(), None);
        assert_eq!(StatusLedState::Running.pattern_ms(), None);
    }
}
