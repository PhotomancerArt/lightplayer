//! The fixture face's power readout.

/// Estimated draw against the supply budget actually in force.
///
/// Present whenever the fixture is limited — including the default 1000 mA
/// guard a fixture gets when it states nothing. Only the explicit zero-budget
/// opt-out hides the readout: unlimited output has no percentage worth
/// showing, and the `power` config slot is where that fixture's owner goes to
/// state a budget again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiFixturePower {
    /// Estimated draw of the last rendered frame, in milliamps.
    ///
    /// An estimate from the lamp type's power model. Nothing here is measured,
    /// and presentation must not imply otherwise.
    pub estimated_draw_ma: u32,
    /// The declared supply budget, in milliamps.
    pub budget_ma: u32,
    /// Output scale currently imposed by limiting, `0.0..=1.0`.
    pub scale: f32,
}

impl UiFixturePower {
    /// Whether output is actively being shed right now.
    pub fn is_limiting(self) -> bool {
        self.scale < 0.999
    }

    /// Estimated draw as a percentage of budget, saturating at `u32::MAX`.
    ///
    /// Reads as a setup number — "you are at 78% of budget" is useful long
    /// before anything goes wrong.
    pub fn percent_of_budget(self) -> u32 {
        if self.budget_ma == 0 {
            return 0;
        }
        (u64::from(self.estimated_draw_ma) * 100 / u64::from(self.budget_ma))
            .try_into()
            .unwrap_or(u32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiting_is_only_true_below_unity() {
        let base = UiFixturePower {
            estimated_draw_ma: 500,
            budget_ma: 1000,
            scale: 1.0,
        };
        assert!(!base.is_limiting());
        assert!(UiFixturePower { scale: 0.5, ..base }.is_limiting());
    }

    #[test]
    fn percent_of_budget_reads_as_a_setup_number() {
        let power = UiFixturePower {
            estimated_draw_ma: 780,
            budget_ma: 1000,
            scale: 1.0,
        };
        assert_eq!(power.percent_of_budget(), 78);
    }

    #[test]
    fn a_zero_budget_does_not_divide() {
        let power = UiFixturePower {
            estimated_draw_ma: 100,
            budget_ma: 0,
            scale: 1.0,
        };
        assert_eq!(power.percent_of_budget(), 0);
    }
}
