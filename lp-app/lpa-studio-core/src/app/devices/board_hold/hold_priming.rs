//! [`HoldPriming`]: a new tab learns what the other tabs hold before its
//! first sweep registers a port.
//!
//! A tab that swept first would open every granted port at load, and the
//! OS would refuse the held ones: "in use by another app", the very words
//! holds exist to replace. So the first sweep waits for one look at the
//! lock manager ([`BoardHoldEdge::held_now`](super::BoardHoldEdge::held_now)):
//! every board hold already taken goes into the book as another tab's, with
//! no level yet (read as the cautious `Open`), and a `Who` asks the holders
//! to say their levels. A look that fails is "nothing held". A look that
//! never answers stops being waited for after [`PRIMING_PATIENCE_SECS`]: a
//! tab with no boards at all is worse than a tab that opened one held port.
//!
//! A hold read here whose holder never says a word is a page that is gone:
//! the old page of a reload, whose Web Lock outlives it for a moment. The
//! ports it accounts for stay shut while it lingers, and when its lock
//! frees they open the way a fresh load opens them (the controller's
//! `hold_freed`, `stale`). A holder that was heard from and later dies never
//! makes this tab open a port.

/// How long the first sweep waits for the lock manager's answer.
pub const PRIMING_PATIENCE_SECS: f64 = 2.0;

/// Where this tab's first look at the lock manager stands.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum HoldPriming {
    /// Not asked yet (the tab's spawner or queue is not installed yet).
    #[default]
    NotStarted,
    /// Asked at `since` (epoch seconds); the sweep waits.
    Waiting { since: f64 },
    /// Answered, or given up on: sweeps run.
    Done,
}

impl HoldPriming {
    /// Whether a sweep may register ports now.
    pub fn lets_sweep_run(self) -> bool {
        matches!(self, Self::Done)
    }

    /// Whether the look has been waited on past [`PRIMING_PATIENCE_SECS`]
    /// at `now`.
    pub fn overdue(self, now: f64) -> bool {
        matches!(self, Self::Waiting { since } if now - since >= PRIMING_PATIENCE_SECS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sweep_waits_for_the_answer_and_not_past_the_patience() {
        assert!(!HoldPriming::NotStarted.lets_sweep_run());
        let waiting = HoldPriming::Waiting { since: 100.0 };
        assert!(!waiting.lets_sweep_run());
        assert!(!waiting.overdue(100.0 + PRIMING_PATIENCE_SECS - 0.1));
        assert!(waiting.overdue(100.0 + PRIMING_PATIENCE_SECS));
        assert!(HoldPriming::Done.lets_sweep_run());
        assert!(!HoldPriming::Done.overdue(1e9));
    }
}
