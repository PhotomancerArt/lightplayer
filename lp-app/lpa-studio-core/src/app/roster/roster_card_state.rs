//! The 14-state roster card vocabulary and its status-line copy.
//!
//! One variant per direction.md "Card grammar" state-table row. The enum
//! is renderer-independent (no web/UI types): the same vocabulary may
//! later drive on-device LEDs and richer displays. Status-line copy lives
//! here so every renderer says the same thing.

use crate::UiStatusKind;
use crate::core::time_ago::time_ago;

use super::roster_affordance::RosterAffordance;
use super::roster_circle::{RosterCircle, RosterCircleShape};

/// Where a roster card (device or live sim runtime) stands, in the
/// honest card vocabulary. Derived by
/// [`derive_roster_card_state`](super::derive_roster_card_state); every
/// variant exists even where not yet reachable live (Degraded has no
/// substrate signal yet; the auto-connect states arrive with M6).
#[derive(Clone, Debug, PartialEq)]
pub enum RosterCardState {
    /// Running the local project's tip. Green solid.
    RunningUpToDate,
    /// Running a version the library has since moved past. Amber solid;
    /// the push button is the D11 consent click.
    RunningBehind {
        /// The device copy's version number on the project line
        /// (`ProjectHistory::version_number` of the observed hash).
        observed_version: Option<usize>,
        /// The local head's version number, for the "Push vN" label.
        head_version: Option<usize>,
    },
    /// Running a copy that is not on the project line — a genuine fork,
    /// already banked at connect (D8). Amber solid; D30 popup resolves.
    EditedOnDevice,
    /// Running, but the device reported crash recovery / safe mode.
    /// Vocabulary slot only in M2: no substrate signal exists yet, so
    /// derivation never produces it (story-covered for the day it does).
    Degraded { reason: DegradedReason },
    /// The connect retry ladder is working (D31 replacement). Amber
    /// pulsing, no affordance — the ladder self-heals or lands in
    /// [`Self::NotResponding`].
    ConnectingRetrying { phase: ConnectPhase },
    /// A long-running operation the user can walk away from (flash,
    /// erase, push). Amber pulsing + progress in the card.
    OperationInFlight {
        /// Human operation label, e.g. "Installing firmware".
        label: String,
        /// Whole-percent progress when the operation reports it.
        percent: Option<u8>,
    },
    /// Live link, nothing loaded. Green solid — an empty device is fine.
    ConnectedEmpty,
    /// Holds project data Studio cannot read (old format, corruption).
    /// Amber solid — honest about the content; replacing (choose a
    /// project) or erasing are the ways out. Added 2026-07-17 after the
    /// hardware walk: mapping this to Connected-empty hid the truth.
    HoldsUnreadableData {
        /// Why the content didn't parse (manifest error detail).
        detail: String,
    },
    /// Blank/erased flash (or ROM download mode): provisioning turns it
    /// into a Device. Amber solid.
    ReadyToSetUp,
    /// Recognized non-LightPlayer firmware, safe to replace. Amber solid.
    OtherFirmware,
    /// Speaks the wire framing but not this build's protocol: reflash is
    /// the only remedy. Amber solid.
    NeedsFirmwareUpdate,
    /// Holds a project but no stamped identity: naming (stamping) adopts
    /// it. Amber solid.
    NeedsAName,
    /// The readiness deadline passed with no classification, or the retry
    /// ladder gave up. Red solid; troubleshooting popup.
    NotResponding,
    /// The port is held by another tab/process. Gray solid; quiet
    /// auto-retry, no affordance.
    InUseElsewhere,
    /// Remembered only — no live link. Gray hollow (the card also fades).
    Offline {
        /// f64 epoch seconds of the last sighting; `None` when the card
        /// comes from a source with no recorded sighting.
        last_seen_at: Option<f64>,
    },
}

/// Why a running device is degraded (no live source yet — Q7).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DegradedReason {
    CrashRecovery,
    SafeMode,
}

/// Which rung of the connect ladder is working.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectPhase {
    Connecting,
    Resetting,
}

impl RosterCardState {
    /// The status circle for this state (direction.md state table).
    ///
    /// Precedence rule: the circle shows the worst ACTIONABLE state;
    /// secondary facts (firmware drift on a Running row) demote to chips —
    /// see [`super::firmware_update_available`].
    pub fn circle(&self) -> RosterCircle {
        let (shape, tone) = match self {
            Self::RunningUpToDate | Self::ConnectedEmpty => {
                (RosterCircleShape::Solid, UiStatusKind::Good)
            }
            Self::RunningBehind { .. }
            | Self::EditedOnDevice
            | Self::Degraded { .. }
            | Self::ReadyToSetUp
            | Self::OtherFirmware
            | Self::NeedsFirmwareUpdate
            | Self::NeedsAName
            | Self::HoldsUnreadableData { .. } => {
                (RosterCircleShape::Solid, UiStatusKind::Attention)
            }
            Self::ConnectingRetrying { .. } | Self::OperationInFlight { .. } => {
                (RosterCircleShape::Pulsing, UiStatusKind::Attention)
            }
            Self::NotResponding => (RosterCircleShape::Solid, UiStatusKind::Error),
            Self::InUseElsewhere => (RosterCircleShape::Solid, UiStatusKind::Neutral),
            Self::Offline { .. } => (RosterCircleShape::Hollow, UiStatusKind::Neutral),
        };
        RosterCircle { shape, tone }
    }

    /// The card's status line (health only — never project names).
    /// `now_secs` feeds the offline "Seen …" recency; other states ignore
    /// it.
    pub fn status_line(&self, now_secs: f64) -> String {
        match self {
            Self::RunningUpToDate => "Running".to_string(),
            Self::RunningBehind {
                observed_version: Some(n),
                ..
            } => format!("Running v{n} — behind your copy"),
            Self::RunningBehind {
                observed_version: None,
                ..
            } => "Running — behind your copy".to_string(),
            Self::EditedOnDevice => "Edited on device".to_string(),
            Self::Degraded {
                reason: DegradedReason::CrashRecovery,
            } => "Recovered from a crash".to_string(),
            Self::Degraded {
                reason: DegradedReason::SafeMode,
            } => "Safe mode".to_string(),
            Self::ConnectingRetrying {
                phase: ConnectPhase::Connecting,
            } => "Connecting…".to_string(),
            Self::ConnectingRetrying {
                phase: ConnectPhase::Resetting,
            } => "Resetting…".to_string(),
            Self::OperationInFlight {
                label,
                percent: Some(p),
            } => format!("{label}… {p}%"),
            Self::OperationInFlight {
                label,
                percent: None,
            } => format!("{label}…"),
            Self::ConnectedEmpty => "Connected — nothing loaded".to_string(),
            Self::HoldsUnreadableData { .. } => "Holds unreadable data".to_string(),
            Self::ReadyToSetUp => "Ready to set up".to_string(),
            Self::OtherFirmware => "Other firmware detected".to_string(),
            Self::NeedsFirmwareUpdate => "Needs a firmware update".to_string(),
            Self::NeedsAName => "Needs a name".to_string(),
            Self::NotResponding => "Not responding".to_string(),
            Self::InUseElsewhere => "In use by another tab".to_string(),
            Self::Offline {
                last_seen_at: Some(then),
            } => format!("Seen {}", time_ago(now_secs, *then)),
            Self::Offline { last_seen_at: None } => "Not seen yet".to_string(),
        }
    }

    /// The card's ≤1 sub-line: the diverged row's banked note (D8 — the
    /// device copy is already saved, nothing is at risk), and the
    /// unreadable row's parse detail.
    pub fn sub_line(&self) -> Option<String> {
        match self {
            Self::EditedOnDevice => Some("Device copy saved to history".to_string()),
            Self::HoldsUnreadableData { detail } => Some(detail.clone()),
            _ => None,
        }
    }

    /// The card's ≤1 affordance (identity only in M2); `None` for the
    /// self-healing states (connecting, operation, in-use-elsewhere).
    pub fn affordance(&self) -> Option<RosterAffordance> {
        match self {
            Self::RunningUpToDate => Some(RosterAffordance::OpenEditor),
            Self::RunningBehind { head_version, .. } => Some(RosterAffordance::PushVersion {
                version: *head_version,
            }),
            Self::EditedOnDevice => Some(RosterAffordance::ResolveDrift),
            Self::Degraded { .. } | Self::NotResponding => Some(RosterAffordance::Troubleshoot),
            Self::ConnectingRetrying { .. }
            | Self::OperationInFlight { .. }
            | Self::InUseElsewhere => None,
            // choosing a project replaces the unreadable content; erase
            // rides the card's actions popover
            Self::ConnectedEmpty | Self::HoldsUnreadableData { .. } => {
                Some(RosterAffordance::ChooseProject)
            }
            Self::ReadyToSetUp | Self::OtherFirmware => Some(RosterAffordance::SetUp),
            Self::NeedsFirmwareUpdate => Some(RosterAffordance::UpdateFirmware),
            Self::NeedsAName => Some(RosterAffordance::NameDevice),
            Self::Offline { .. } => Some(RosterAffordance::Reconnect),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circles_follow_the_direction_table() {
        let solid = |tone| RosterCircle {
            shape: RosterCircleShape::Solid,
            tone,
        };
        assert_eq!(
            RosterCardState::RunningUpToDate.circle(),
            solid(UiStatusKind::Good)
        );
        assert_eq!(
            RosterCardState::ConnectedEmpty.circle(),
            solid(UiStatusKind::Good)
        );
        assert_eq!(
            RosterCardState::NotResponding.circle(),
            solid(UiStatusKind::Error)
        );
        assert_eq!(
            RosterCardState::InUseElsewhere.circle(),
            solid(UiStatusKind::Neutral)
        );
        assert_eq!(
            RosterCardState::Offline { last_seen_at: None }.circle(),
            RosterCircle {
                shape: RosterCircleShape::Hollow,
                tone: UiStatusKind::Neutral,
            }
        );
        // every working state pulses amber (the attention family)
        for working in [
            RosterCardState::ConnectingRetrying {
                phase: ConnectPhase::Resetting,
            },
            RosterCardState::OperationInFlight {
                label: "Installing firmware".to_string(),
                percent: Some(62),
            },
        ] {
            assert_eq!(
                working.circle(),
                RosterCircle {
                    shape: RosterCircleShape::Pulsing,
                    tone: UiStatusKind::Attention,
                }
            );
        }
        // the attention family is amber solid
        for attention in [
            RosterCardState::RunningBehind {
                observed_version: Some(3),
                head_version: Some(4),
            },
            RosterCardState::EditedOnDevice,
            RosterCardState::Degraded {
                reason: DegradedReason::SafeMode,
            },
            RosterCardState::ReadyToSetUp,
            RosterCardState::OtherFirmware,
            RosterCardState::NeedsFirmwareUpdate,
            RosterCardState::NeedsAName,
        ] {
            assert_eq!(attention.circle(), solid(UiStatusKind::Attention));
        }
    }

    #[test]
    fn status_lines_speak_the_direction_copy() {
        let now = 1_000_000.0;
        assert_eq!(RosterCardState::RunningUpToDate.status_line(now), "Running");
        assert_eq!(
            RosterCardState::RunningBehind {
                observed_version: Some(3),
                head_version: Some(5),
            }
            .status_line(now),
            "Running v3 — behind your copy"
        );
        assert_eq!(
            RosterCardState::OperationInFlight {
                label: "Installing firmware".to_string(),
                percent: Some(62),
            }
            .status_line(now),
            "Installing firmware… 62%"
        );
        assert_eq!(
            RosterCardState::Offline {
                last_seen_at: Some(now - 2.0 * 86_400.0),
            }
            .status_line(now),
            "Seen 2d ago"
        );
        assert_eq!(
            RosterCardState::ConnectedEmpty.status_line(now),
            "Connected — nothing loaded"
        );
    }

    #[test]
    fn only_the_diverged_row_carries_the_banked_sub_line() {
        assert!(RosterCardState::EditedOnDevice.sub_line().is_some());
        assert!(RosterCardState::RunningUpToDate.sub_line().is_none());
        assert!(RosterCardState::NotResponding.sub_line().is_none());
    }

    #[test]
    fn affordances_match_the_direction_table() {
        assert_eq!(
            RosterCardState::RunningBehind {
                observed_version: Some(3),
                head_version: Some(5),
            }
            .affordance(),
            Some(RosterAffordance::PushVersion { version: Some(5) })
        );
        assert_eq!(
            RosterCardState::RunningBehind {
                observed_version: Some(3),
                head_version: Some(5),
            }
            .affordance()
            .unwrap()
            .label(),
            "Push v5"
        );
        // the self-healing states offer nothing
        for quiet in [
            RosterCardState::ConnectingRetrying {
                phase: ConnectPhase::Connecting,
            },
            RosterCardState::OperationInFlight {
                label: "Pushing".to_string(),
                percent: None,
            },
            RosterCardState::InUseElsewhere,
        ] {
            assert_eq!(quiet.affordance(), None);
        }
        assert_eq!(
            RosterCardState::ReadyToSetUp.affordance(),
            Some(RosterAffordance::SetUp)
        );
        assert_eq!(
            RosterCardState::OtherFirmware.affordance(),
            Some(RosterAffordance::SetUp)
        );
        assert_eq!(
            RosterCardState::Offline { last_seen_at: None }.affordance(),
            Some(RosterAffordance::Reconnect)
        );
    }
}
