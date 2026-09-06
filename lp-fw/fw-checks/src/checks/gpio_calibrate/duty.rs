//! The duty ramp: a soft-timed square wave whose duty cycle walks 10 % up to
//! 90 % and back, one step every 200 ms.
//!
//! Pure arithmetic over microseconds. The firmware supplies the clock; this
//! decides what the pin should be doing. Splitting it that way is what makes
//! the interesting behaviour — the ramp turning around at its endpoints, the
//! level flipping at the right point of each 4 ms period — testable without a
//! chip.

/// One full square-wave period.
pub const DUTY_PERIOD_US: u64 = 4_000;
pub const DUTY_MIN_PERCENT: u8 = 10;
pub const DUTY_MAX_PERCENT: u8 = 90;
pub const DUTY_STEP_PERCENT: u8 = 10;

/// The ramp's state: where the duty cycle is, and which way it is walking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DutyRamp {
    percent: u8,
    rising: bool,
}

impl Default for DutyRamp {
    fn default() -> Self {
        Self::new()
    }
}

impl DutyRamp {
    pub const fn new() -> Self {
        Self {
            percent: DUTY_MIN_PERCENT,
            rising: true,
        }
    }

    pub const fn percent(self) -> u8 {
        self.percent
    }

    pub const fn rising(self) -> bool {
        self.rising
    }

    /// Back to the start. Called when a new pin is opened.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// One `DUTY_STEP_INTERVAL` has elapsed: advance, turning around at the
    /// endpoints.
    pub fn step(&mut self) {
        if self.rising {
            self.percent = self.percent.saturating_add(DUTY_STEP_PERCENT);
            if self.percent >= DUTY_MAX_PERCENT {
                self.percent = DUTY_MAX_PERCENT;
                self.rising = false;
            }
        } else {
            self.percent = self.percent.saturating_sub(DUTY_STEP_PERCENT);
            if self.percent <= DUTY_MIN_PERCENT {
                self.percent = DUTY_MIN_PERCENT;
                self.rising = true;
            }
        }
    }

    /// Should the pin be high, this many microseconds after the pulse started?
    pub const fn level_high(self, elapsed_us: u64) -> bool {
        let phase = elapsed_us % DUTY_PERIOD_US;
        let high_us = DUTY_PERIOD_US * self.percent as u64 / 100;
        phase < high_us
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::vec::Vec;

    use super::*;

    #[test]
    fn starts_at_the_minimum_rising() {
        let r = DutyRamp::new();
        assert_eq!(r.percent(), DUTY_MIN_PERCENT);
        assert!(r.rising());
    }

    #[test]
    fn walks_up_to_the_maximum_and_back_down_forever() {
        let mut r = DutyRamp::new();
        let mut seen = Vec::new();
        for _ in 0..32 {
            seen.push(r.percent());
            r.step();
        }
        assert_eq!(
            seen,
            [
                10, 20, 30, 40, 50, 60, 70, 80, 90, // up
                80, 70, 60, 50, 40, 30, 20, 10, // and back
                20, 30, 40, 50, 60, 70, 80, 90, // and again
                80, 70, 60, 50, 40, 30, 20,
            ]
        );
    }

    #[test]
    fn never_leaves_its_bounds() {
        let mut r = DutyRamp::new();
        for _ in 0..1000 {
            assert!((DUTY_MIN_PERCENT..=DUTY_MAX_PERCENT).contains(&r.percent()));
            r.step();
        }
    }

    #[test]
    fn reset_goes_back_to_the_start() {
        let mut r = DutyRamp::new();
        for _ in 0..5 {
            r.step();
        }
        assert_ne!(r, DutyRamp::new());
        r.reset();
        assert_eq!(r, DutyRamp::new());
    }

    #[test]
    fn level_flips_at_the_duty_point_of_every_period() {
        let mut r = DutyRamp::new(); // 10 % of 4,000 us = 400 us high
        assert!(r.level_high(0));
        assert!(r.level_high(399));
        assert!(!r.level_high(400));
        assert!(!r.level_high(3_999));
        // …and the same in the next period.
        assert!(r.level_high(4_000));
        assert!(!r.level_high(4_400));

        for _ in 0..8 {
            r.step();
        }
        assert_eq!(r.percent(), 90); // 3,600 us high
        assert!(r.level_high(3_599));
        assert!(!r.level_high(3_600));
    }

    #[test]
    fn duty_is_never_the_full_period_at_either_end() {
        for percent in [DUTY_MIN_PERCENT, DUTY_MAX_PERCENT] {
            let mut r = DutyRamp::new();
            while r.percent() != percent {
                r.step();
            }
            assert!(r.level_high(0), "{percent}% should start high");
            assert!(
                !r.level_high(DUTY_PERIOD_US - 1),
                "{percent}% should end low"
            );
        }
    }
}
