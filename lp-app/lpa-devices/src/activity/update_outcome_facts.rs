//! How an update ended — the model's mirror of `lpa_update::Finish` (and its
//! `StopReason`, flattened), plus the one ending only the model can see.
//!
//! Flat on purpose: the card's words (`lpa-studio-core`) and the view match
//! on it directly, with no driver type in sight.

use serde::{Deserialize, Serialize};

use super::activity_cell::ActivityOutcome;

/// What an [`ActivityMarker::UpdateOutcome`](crate::ActivityMarker::UpdateOutcome)
/// reports, and what an ended Update activity leaves on the device
/// ([`crate::Evidence::last_update_outcome`]).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum UpdateOutcomeFacts {
    /// The board runs the build the update asked for (or already did).
    UpToDate,
    /// The board cannot update over this link yet (its loader or layout
    /// needs one update over USB).
    NeedsUsb,
    /// The board runs a newer version than the one on offer.
    BoardIsNewer,
    /// The board's engine keeps crashing; nothing was asked to fix it.
    Crashing,
    /// Another device holds the board's transfer.
    Busy,
    /// The person's access on this board is play only.
    PlayOnly,
    /// The board refuses that build (it failed its trial here).
    RefusedBuild,
    /// The board is another target than any build this Studio has.
    OtherTarget,
    /// The board asked for a login and no credentials were at hand.
    NoCredentials,
    /// The running engine refused: log in on the board's own link first.
    NeedsEngineLogin,
    /// Every credential was refused.
    LoginRefused,
    /// No source had the engine the board needs; `offline` when the store
    /// was not reachable.
    MissingEngine { offline: bool },
    /// The read-back did not hash to the engine the board reports.
    BackupFailed,
    /// A piece failed its check more times than allowed.
    TooManyRetries,
    /// The board refused, and retrying would not change it.
    Refused,
    /// The board's firmware is older than a message the update sent.
    BoardLacksMessage,
    /// Over Wi‑Fi, the board said nothing on the update channel when asked
    /// (a release from before updates over Wi‑Fi: its LAN link ignores the
    /// channel its hello announces). It updates over USB or Bluetooth until
    /// it has been updated once.
    NotOverWifi,
    /// The model's own ending: the board reset (or its link dropped)
    /// between legs and did not come back within the gap's deadline. It
    /// keeps its place — reconnecting it finishes the update.
    BoardDidNotComeBack,
}

impl UpdateOutcomeFacts {
    /// Whether the update reached its goal.
    pub fn is_up_to_date(self) -> bool {
        matches!(self, Self::UpToDate)
    }

    /// The model's own plain words (the card's are the app layer's).
    pub fn describe(self) -> &'static str {
        match self {
            Self::UpToDate => "firmware up to date",
            Self::NeedsUsb => "this board needs one update over USB",
            Self::BoardIsNewer => "the board runs a newer version than this Studio",
            Self::Crashing => "the board's firmware keeps crashing",
            Self::Busy => "another device is updating this board",
            Self::PlayOnly => "updating this board needs the author password",
            Self::RefusedBuild => "the board refuses this build",
            Self::OtherTarget => "this Studio has no firmware for this board",
            Self::NoCredentials => "the board asked for a login this Studio holds no key for",
            Self::NeedsEngineLogin => "log in to the board, then update it",
            Self::LoginRefused => "the board refused the login",
            Self::MissingEngine { offline: true } => {
                "no copy of the board's firmware here, and the store is not reachable"
            }
            Self::MissingEngine { offline: false } => {
                "no copy of the board's firmware here or online"
            }
            Self::BackupFailed => "backing up the current firmware failed",
            Self::TooManyRetries => "the update failed its checks too many times",
            Self::Refused => "the board refused the update",
            Self::BoardLacksMessage => "the board's firmware is too old for this update",
            Self::NotOverWifi => "this board doesn't update over Wi\u{2011}Fi yet",
            Self::BoardDidNotComeBack => {
                "the board did not come back — it keeps its place; reconnect it to finish"
            }
        }
    }

    /// The generic outcome the activity's bracket carries (the banner, the
    /// journal). Up to date is the one success; every other ending is a
    /// "not updated" with its reason.
    pub fn activity_outcome(self) -> ActivityOutcome {
        match self {
            Self::UpToDate => ActivityOutcome::Succeeded {
                summary: self.describe().to_string(),
            },
            other => ActivityOutcome::Failed {
                message: other.describe().to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_up_to_date_is_a_success() {
        assert!(UpdateOutcomeFacts::UpToDate.activity_outcome().is_success());
        for outcome in [
            UpdateOutcomeFacts::NeedsUsb,
            UpdateOutcomeFacts::MissingEngine { offline: true },
            UpdateOutcomeFacts::BoardDidNotComeBack,
        ] {
            let ended = outcome.activity_outcome();
            assert!(!ended.is_success(), "{outcome:?}");
            assert!(!ended.summary().is_empty());
        }
    }

    #[test]
    fn outcomes_round_trip_through_json() {
        let outcome = UpdateOutcomeFacts::MissingEngine { offline: true };
        let json = serde_json::to_string(&outcome).expect("serialize");
        let back: UpdateOutcomeFacts = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, outcome);
    }
}
