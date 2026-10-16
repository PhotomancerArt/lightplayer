//! How each board's last activity ended, and when: what the board card's
//! bars read to go green for a few seconds after work that went well, or to
//! stay striped with Retry after work that failed (D34).
//!
//! The model keeps a finished activity's outcome (`DeviceView::last_outcome`,
//! a summary and whether it went well) but neither its kind nor its time, so
//! a bar could not tell its own ending from another bar's, nor when it was.
//! The device sub-controller keeps this record beside the model instead —
//! no `lpa-devices` change — read off the roster's journal as each fold
//! drains it: an `ActivityEnded` note names the kind, the outcome and the
//! instant, even when the next activity starts in the same fold (where the
//! model has already cleared `last_outcome` again).
//!
//! A cancel is not an ending a bar reports: the person asked for it, and the
//! bar simply goes back to what it said before.

use std::collections::{BTreeMap, BTreeSet};

use lpa_devices::journal::JournalNote;
use lpa_devices::time::Millis;
use lpa_devices::{ActivityKind, ActivityOutcome, DeviceId};

/// How long a bar stays green after its work ended well.
pub const DONE_SHOWS_SECS: f64 = 3.0;

/// One board's last activity's end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActivityEnd {
    /// What the activity was: which bar it belonged to.
    pub kind: ActivityKind,
    /// It ended well.
    pub ok: bool,
    /// When it ended, in epoch seconds (the controller's clock).
    pub at: f64,
}

impl ActivityEnd {
    /// Whether a bar still shows this end as Done at `now`: an end that went
    /// well, less than [`DONE_SHOWS_SECS`] ago.
    pub fn shows_done(&self, now: f64) -> bool {
        self.ok && now >= self.at && now - self.at < DONE_SHOWS_SECS
    }
}

/// Every board's last activity end. See the module docs.
#[derive(Clone, Debug, Default)]
pub struct ActivityEnds {
    ends: BTreeMap<DeviceId, ActivityEnd>,
    /// Boards whose Done window is still open, as far as the last ask of
    /// [`Self::done_lapsed`] knows.
    showing_done: BTreeSet<DeviceId>,
}

impl ActivityEnds {
    /// Take one journal note the fold wrote for `device` at `at`: an end is
    /// recorded, a cancel forgets the board's last end.
    pub fn note(&mut self, device: DeviceId, note: &JournalNote, at: Millis) {
        let at = at.0 as f64 / 1_000.0;
        let (kind, ok) = match note {
            JournalNote::ActivityEnded {
                outcome: ActivityOutcome::Cancelled,
                ..
            } => {
                self.ends.remove(&device);
                self.showing_done.remove(&device);
                return;
            }
            JournalNote::ActivityEnded { kind, outcome } => (*kind, outcome.is_success()),
            // Removed rather than finished (its grace or deadline ran out, its
            // link went): it did not end well.
            JournalNote::ActivityEvicted { kind, .. } => (*kind, false),
            _ => return,
        };
        self.ends.insert(device, ActivityEnd { kind, ok, at });
        match ok {
            true => self.showing_done.insert(device),
            false => self.showing_done.remove(&device),
        };
    }

    /// `device`'s last end, when it has had one.
    pub fn get(&self, device: DeviceId) -> Option<&ActivityEnd> {
        self.ends.get(&device)
    }

    /// Every board's last end.
    pub fn all(&self) -> &BTreeMap<DeviceId, ActivityEnd> {
        &self.ends
    }

    /// Forget the boards `keep` lets go of (forgotten, merged away).
    pub fn retain(&mut self, keep: impl Fn(DeviceId) -> bool) {
        self.ends.retain(|device, _| keep(*device));
        self.showing_done.retain(|device| keep(*device));
    }

    /// Whether a Done window closed since the last ask: the view that
    /// showed it green must be published again without it. Asked once per
    /// view, like the agent's lights going dark.
    pub fn done_lapsed(&mut self, now: f64) -> bool {
        let ends = &self.ends;
        let before = self.showing_done.len();
        self.showing_done
            .retain(|device| ends.get(device).is_some_and(|end| end.shows_done(now)));
        self.showing_done.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_end_that_went_well_shows_done_for_three_seconds() {
        let mut ends = ActivityEnds::default();
        let board = DeviceId(1);
        ends.note(board, &ended(ActivityKind::Push, true), Millis(10_000));
        let end = *ends.get(board).expect("recorded");
        assert_eq!(end.kind, ActivityKind::Push);
        assert!(end.ok);
        assert_eq!(end.at, 10.0);
        assert!(end.shows_done(10.0));
        assert!(end.shows_done(12.9));
        assert!(!end.shows_done(13.0), "gone after three seconds");

        assert!(!ends.done_lapsed(11.0), "still showing");
        assert!(ends.done_lapsed(13.5), "the green went out: publish again");
        assert!(!ends.done_lapsed(14.0), "said once");
    }

    #[test]
    fn a_failed_end_stays_and_never_shows_done() {
        let mut ends = ActivityEnds::default();
        let board = DeviceId(1);
        ends.note(board, &ended(ActivityKind::Flash, false), Millis(5_000));
        let end = *ends.get(board).expect("recorded");
        assert!(!end.ok);
        assert!(!end.shows_done(5.0));
        assert!(!ends.done_lapsed(100.0), "a failure has no window to lapse");
        assert_eq!(ends.get(board), Some(&end), "it stays until superseded");

        ends.note(board, &ended(ActivityKind::Flash, true), Millis(9_000));
        assert!(ends.get(board).unwrap().ok, "the next end supersedes it");
    }

    #[test]
    fn an_evicted_activity_ended_badly_and_a_cancel_forgets() {
        let mut ends = ActivityEnds::default();
        let board = DeviceId(2);
        ends.note(
            board,
            &JournalNote::ActivityEvicted {
                kind: ActivityKind::Push,
                reason: lpa_devices::journal::EvictionReason::LinkLost,
            },
            Millis(1_000),
        );
        assert!(!ends.get(board).unwrap().ok);
        ends.note(
            board,
            &JournalNote::ActivityEnded {
                kind: ActivityKind::Push,
                outcome: ActivityOutcome::Cancelled,
            },
            Millis(2_000),
        );
        assert_eq!(ends.get(board), None, "a cancel is no failure to stripe");
    }

    #[test]
    fn other_notes_and_forgotten_boards_leave_no_end() {
        let mut ends = ActivityEnds::default();
        ends.note(
            DeviceId(1),
            &JournalNote::ActivityStarted {
                kind: ActivityKind::Push,
            },
            Millis(1_000),
        );
        assert!(ends.all().is_empty());
        ends.note(DeviceId(1), &ended(ActivityKind::Push, true), Millis(1_000));
        ends.retain(|device| device != DeviceId(1));
        assert!(ends.all().is_empty());
        assert!(!ends.done_lapsed(100.0), "nothing left to lapse");
    }

    fn ended(kind: ActivityKind, ok: bool) -> JournalNote {
        JournalNote::ActivityEnded {
            kind,
            outcome: match ok {
                true => ActivityOutcome::Succeeded {
                    summary: "done".to_string(),
                },
                false => ActivityOutcome::Failed {
                    message: "it broke".to_string(),
                },
            },
        }
    }
}
