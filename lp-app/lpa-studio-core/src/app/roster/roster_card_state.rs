//! The roster card vocabulary and its status-line copy.
//!
//! One variant per direction.md "Card grammar" state-table row, plus the
//! rows later work has had to add (the 2026-07-31 recovery-mode split, the
//! 2026-08-04 format card). This file's doc comments are the de-facto card
//! spec: a variant explains what the state IS, why it exists as its own
//! row, and what the card offers there. The enum
//! is renderer-independent (no web/UI types): the same vocabulary may
//! later drive on-device LEDs and richer displays. Status-line copy lives
//! here so every renderer says the same thing.

use crate::UiStatusKind;
use crate::core::time_ago::time_ago;

use super::roster_affordance::RosterAffordance;
use super::roster_state_spec::{RosterStateSpec, RosterTreatment};

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
    /// Running a copy that is not on the project line — a genuine fork
    /// (auto-fast-forward already absorbed pure extensions, §3c-1),
    /// already banked at connect (D8). Amber solid; the two resolve verbs
    /// ride the card face (§3c-2 — the D30 sheet is gone).
    EditedOnDevice {
        /// When the local head was last saved (plain-words comparison).
        local_saved_at: Option<f64>,
        /// When we last pushed to this board (its registry association).
        pushed_at: Option<f64>,
    },
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
    /// Holds project data Studio cannot read at all — corruption, a
    /// missing manifest, a failed read. Amber solid — honest about the
    /// content; wiping back to blank is the way out. Added 2026-07-17
    /// after the hardware walk: mapping this to Connected-empty hid the
    /// truth.
    ///
    /// A project that reads fine but states a format this build does not
    /// use is NOT this state — it is
    /// [`Self::HoldsOldFormatProject`], which can say what was found and,
    /// where the migration chain reaches, offer one click that fixes it.
    HoldsUnreadableData {
        /// Why the content didn't parse (manifest error detail).
        detail: String,
    },
    /// Holds a readable project whose FORMAT is not this build's. Amber
    /// solid.
    ///
    /// Added 2026-08-04 (project-format-upgrades P5). Before it, a board
    /// holding a format-4 project classified as Running/Known — the pull
    /// read the manifest's `uid` and never its `format` — so the card
    /// claimed a board was running a project its firmware had refused to
    /// load, and the only honest-looking way out was the wipe on
    /// [`Self::HoldsUnreadableData`]. This state says which format was
    /// found and which this Studio uses, and carries the one verb that
    /// resolves it.
    ///
    /// Despite the name it covers BOTH directions of format drift — a
    /// board left behind by a Studio update (the common case) and one
    /// written by a NEWER LightPlayer. [`DeviceFormatStanding`] carries
    /// which, because the two take different verbs: only an upgradable
    /// format earns [`RosterAffordance::UpgradeProject`]; the rest keep
    /// the wipe plus a note naming what to do instead.
    ///
    /// The board's bytes are already banked in the library when this card
    /// appears (connect-is-a-pull, D8), which is what makes upgrading —
    /// and wiping — survivable.
    HoldsOldFormatProject {
        /// What the manifest sniff found, and whether this build can
        /// migrate it.
        standing: DeviceFormatStanding,
        /// The format this build writes (`PROJECT_FORMAT_VERSION`),
        /// carried as evidence so the copy layer stays pure.
        expected: u32,
    },
    /// Blank/erased flash: provisioning turns it into a Device. Amber
    /// solid.
    ReadyToSetUp,
    /// The chip is sitting in ROM download mode. Amber solid.
    ///
    /// Split out of [`Self::ReadyToSetUp`] 2026-07-31 (bench report): the
    /// two were collapsed, so Studio detected download mode and then threw
    /// the fact away. They are not the same situation and they do not want
    /// the same verbs.
    ///
    /// Users arrive here three ways — a new board that will not talk
    /// normally, a board whose existing firmware interferes with flashing,
    /// or a device they are trying to rescue — but the verbs are the same
    /// for all three, so this presents ONE flow rather than branching.
    ///
    /// The load-bearing difference from `ReadyToSetUp`: **a device flashed
    /// from here does not boot the new firmware on its own.** It has to be
    /// physically unplugged and replugged. So the recovery flash ends on an
    /// instruction, not on an auto-reconnect that would fail and report a
    /// successful flash as a failure.
    RecoveryMode,
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

/// Where a board's project format stands relative to this build — the
/// card's own vocabulary for [`lpa_upgrade::FormatClass`], carrying only
/// the distinctions the card acts on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviceFormatStanding {
    /// Old, and within the migration floor: one click fixes it.
    Upgradable { found: u32 },
    /// Older than the floor — the types those formats named are deleted,
    /// so no automatic path exists. `found` is `None` for a pre-mitosis
    /// project, which states no version at all.
    TooOld { found: Option<u32> },
    /// Written by a newer LightPlayer than this build. Not a defect in the
    /// project: this Studio is the one that is behind.
    FromNewerStudio { found: u32 },
}

impl DeviceFormatStanding {
    /// The version the manifest stated, when it stated one.
    pub fn found(&self) -> Option<u32> {
        match self {
            Self::Upgradable { found } | Self::FromNewerStudio { found } => Some(*found),
            Self::TooOld { found } => *found,
        }
    }

    /// Whether this build can migrate it — the gate on the Upgrade verb.
    pub fn is_upgradable(&self) -> bool {
        matches!(self, Self::Upgradable { .. })
    }
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
    /// The state's presentation spec (direction.md state table) — the
    /// retired circle's grammar, carried by the card's tint edge now.
    ///
    /// Precedence rule: the spec shows the worst ACTIONABLE state;
    /// secondary facts (firmware drift on a Running row) demote to chips —
    /// see [`super::firmware_update_available`].
    pub fn spec(&self) -> RosterStateSpec {
        let (treatment, tone) = match self {
            Self::RunningUpToDate | Self::ConnectedEmpty => {
                (RosterTreatment::Filled, UiStatusKind::Good)
            }
            Self::RunningBehind { .. }
            | Self::EditedOnDevice { .. }
            | Self::Degraded { .. }
            | Self::ReadyToSetUp
            | Self::RecoveryMode
            | Self::OtherFirmware
            | Self::NeedsFirmwareUpdate
            | Self::NeedsAName
            | Self::HoldsUnreadableData { .. }
            | Self::HoldsOldFormatProject { .. } => {
                (RosterTreatment::Filled, UiStatusKind::Attention)
            }
            Self::ConnectingRetrying { .. } | Self::OperationInFlight { .. } => {
                (RosterTreatment::Working, UiStatusKind::Attention)
            }
            Self::NotResponding => (RosterTreatment::Filled, UiStatusKind::Error),
            Self::InUseElsewhere => (RosterTreatment::Filled, UiStatusKind::Neutral),
            Self::Offline { .. } => (RosterTreatment::Remembered, UiStatusKind::Neutral),
        };
        RosterStateSpec { treatment, tone }
    }

    /// The card's status line (health only — never project names).
    /// `now_secs` feeds the offline "Seen …" recency; other states ignore
    /// it.
    pub fn status_line(&self, now_secs: f64) -> String {
        match self {
            Self::RunningUpToDate => "Running".to_string(),
            // §3a copy rule: no version jargon in the headline — the
            // Project facts carry the save-distance in plain words.
            Self::RunningBehind { .. } => "Running an older version".to_string(),
            Self::EditedOnDevice { .. } => "Changed on the device".to_string(),
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
            // §3a: the headline states the FACT (which format is on the
            // board); the sub-line explains what that means and what to
            // do. "Holds an incompatible project" would be a label the
            // user cannot check.
            Self::HoldsOldFormatProject { standing, .. } => match standing.found() {
                Some(found) => format!("Holds a format-{found} project"),
                None => "Holds a project with no stated format".to_string(),
            },
            Self::ReadyToSetUp => "Ready to set up".to_string(),
            // "Bootloader" is our word, not the user's; the technical term
            // stays available on the rich object.
            Self::RecoveryMode => "Recovery mode".to_string(),
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
    /// unreadable row's parse detail. `now_secs` feeds the drift times
    /// (§3c-3); states without time copy ignore it.
    pub fn sub_line(&self, now_secs: f64) -> Option<String> {
        match self {
            // §3a again: the label alone leaves the user guessing why a
            // board they just plugged in is not simply working. Say what
            // this state IS, and say the part that surprises people — a
            // device flashed from here does not come back on its own.
            Self::RecoveryMode => Some(
                "This board is waiting to be re-flashed instead of running \
                 its firmware — usually a new board that won't talk normally, \
                 or one being rescued. Anything you install from here needs \
                 the board unplugged and plugged back in before it will run."
                    .to_string(),
            ),
            // §3a: explain the situation, not just the label — with the
            // honest wall-clock facts when we have them (§3c-3).
            Self::EditedOnDevice {
                local_saved_at,
                pushed_at,
            } => {
                let pushed = pushed_at
                    .map(|at| format!(" ({})", time_ago(now_secs, at)))
                    .unwrap_or_default();
                let saved = local_saved_at
                    .map(|at| format!(" Your copy was saved {}.", time_ago(now_secs, at)))
                    .unwrap_or_default();
                Some(format!(
                    "This board's copy has edits your project doesn't — made \
                     after your last push{pushed}.{saved} A backup is already \
                     in your library."
                ))
            }
            Self::HoldsUnreadableData { detail } => Some(detail.clone()),
            // Every arm names a remedy: a classification the user cannot
            // act on is the silent-failure problem in a new costume
            // (the rule `FormatClass::describe` follows, in card voice).
            Self::HoldsOldFormatProject { standing, expected } => Some(match standing {
                DeviceFormatStanding::Upgradable { .. } => format!(
                    "Studio uses format {expected}, so this board isn't running it. \
                     Upgrading rewrites the project in your library and puts it back \
                     on the board — the copy that's there now is already saved."
                ),
                DeviceFormatStanding::TooOld { found: Some(found) } => format!(
                    "Studio uses format {expected}, and format {found} is too old to \
                     upgrade automatically. Open it in a LightPlayer that still reads \
                     format {found} and re-save it, or wipe the board and push a \
                     project from your library."
                ),
                DeviceFormatStanding::TooOld { found: None } => format!(
                    "Studio uses format {expected}. This project predates the version \
                     stamp, so there is no automatic path forward — rebuild it, or \
                     wipe the board and push a project from your library."
                ),
                DeviceFormatStanding::FromNewerStudio { found } => format!(
                    "Format {found} was made by a newer LightPlayer than this one \
                     (it writes format {expected}). Update LightPlayer to work with \
                     it, or wipe the board and push a project from your library."
                ),
            }),
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
            // §3c-2: the clear thing to do leads — adopt is
            // overwrite-with-history, so it needs no gate; Keep-both rides
            // beside it on the face (rich-object chain).
            Self::EditedOnDevice { .. } => Some(RosterAffordance::UseBoardCopy),
            Self::Degraded { .. } | Self::NotResponding => Some(RosterAffordance::Troubleshoot),
            Self::ConnectingRetrying { .. }
            | Self::OperationInFlight { .. }
            | Self::InUseElsewhere => None,
            Self::ConnectedEmpty => Some(RosterAffordance::ChooseProject),
            // The way out is BLANK, never push-over (model rev 2026-07-26):
            // wipe the unreadable content, land on "nothing loaded".
            Self::HoldsUnreadableData { .. } => Some(RosterAffordance::WipeProject),
            // The one verb that resolves the situation, where it exists:
            // upgrade (pull → migrate in the library → push). Below the
            // floor or ahead of this build there is no such verb, so the
            // card offers the honest way out instead of a button that
            // would only fail.
            Self::HoldsOldFormatProject { standing, .. } => Some(if standing.is_upgradable() {
                RosterAffordance::UpgradeProject
            } else {
                RosterAffordance::WipeProject
            }),
            Self::ReadyToSetUp | Self::OtherFirmware => Some(RosterAffordance::SetUp),
            // Not SetUp: this is not a normal provisioning. The recovery
            // flash has a different ending (replug), and a device here may
            // hold data the user came to rescue — so the headline verb must
            // not be the one that erases it.
            Self::RecoveryMode => Some(RosterAffordance::Troubleshoot),
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
        let solid = |tone| RosterStateSpec {
            treatment: RosterTreatment::Filled,
            tone,
        };
        assert_eq!(
            RosterCardState::RunningUpToDate.spec(),
            solid(UiStatusKind::Good)
        );
        assert_eq!(
            RosterCardState::ConnectedEmpty.spec(),
            solid(UiStatusKind::Good)
        );
        assert_eq!(
            RosterCardState::NotResponding.spec(),
            solid(UiStatusKind::Error)
        );
        assert_eq!(
            RosterCardState::InUseElsewhere.spec(),
            solid(UiStatusKind::Neutral)
        );
        assert_eq!(
            RosterCardState::Offline { last_seen_at: None }.spec(),
            RosterStateSpec {
                treatment: RosterTreatment::Remembered,
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
                working.spec(),
                RosterStateSpec {
                    treatment: RosterTreatment::Working,
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
            RosterCardState::EditedOnDevice {
                local_saved_at: None,
                pushed_at: None,
            },
            RosterCardState::Degraded {
                reason: DegradedReason::SafeMode,
            },
            RosterCardState::ReadyToSetUp,
            RosterCardState::OtherFirmware,
            RosterCardState::NeedsFirmwareUpdate,
            RosterCardState::NeedsAName,
        ] {
            assert_eq!(attention.spec(), solid(UiStatusKind::Attention));
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
            "Running an older version"
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
    fn recovery_mode_explains_itself_and_warns_about_the_replug() {
        let note = RosterCardState::RecoveryMode.sub_line(0.0).expect(
            "recovery mode must explain itself — the label alone \
                     leaves the user guessing",
        );
        // The replug is the part that surprises people: without it a
        // successful flash looks like a dead board.
        assert!(
            note.contains("unplugged") && note.contains("plugged back in"),
            "the replug requirement must be stated: {note}"
        );
    }

    #[test]
    fn only_the_diverged_row_carries_the_banked_sub_line() {
        let now = 1_000_000.0;
        assert!(
            RosterCardState::EditedOnDevice {
                local_saved_at: None,
                pushed_at: None,
            }
            .sub_line(now)
            .is_some()
        );
        assert!(RosterCardState::RunningUpToDate.sub_line(now).is_none());
        assert!(RosterCardState::NotResponding.sub_line(now).is_none());
    }

    #[test]
    fn the_diverged_sub_line_speaks_the_drift_times() {
        let now = 1_000_000.0;
        let line = RosterCardState::EditedOnDevice {
            local_saved_at: Some(now - 240.0),
            pushed_at: Some(now - 7_200.0),
        }
        .sub_line(now)
        .expect("diverged sub-line");
        assert!(
            line.contains("your last push (2h ago)"),
            "push recency in plain words: {line}"
        );
        assert!(
            line.contains("Your copy was saved 4m ago."),
            "local save recency in plain words: {line}"
        );
    }

    /// P5's gate: the Upgrade button appears ONLY where the migration
    /// chain actually reaches. A verb that would refuse the moment it was
    /// pressed is worse than the honest way out.
    #[test]
    fn only_an_upgradable_format_offers_the_upgrade_verb() {
        let card = |standing| RosterCardState::HoldsOldFormatProject {
            standing,
            expected: 5,
        };
        assert_eq!(
            card(DeviceFormatStanding::Upgradable { found: 4 }).affordance(),
            Some(RosterAffordance::UpgradeProject)
        );
        for refused in [
            DeviceFormatStanding::TooOld { found: Some(2) },
            DeviceFormatStanding::TooOld { found: None },
            DeviceFormatStanding::FromNewerStudio { found: 99 },
        ] {
            assert_eq!(
                card(refused.clone()).affordance(),
                Some(RosterAffordance::WipeProject),
                "{refused:?} has no upgrade path — the card must not pretend it does"
            );
        }
    }

    /// The card states the FACT (which format is on the board) and then
    /// what to do about it — every arm, including the ones with no
    /// upgrade path. A classification the user cannot act on is the
    /// silent-failure problem in a new costume.
    #[test]
    fn the_format_card_names_what_it_found_and_a_remedy() {
        let upgradable = RosterCardState::HoldsOldFormatProject {
            standing: DeviceFormatStanding::Upgradable { found: 4 },
            expected: 5,
        };
        assert_eq!(upgradable.status_line(0.0), "Holds a format-4 project");
        let note = upgradable.sub_line(0.0).expect("a note");
        assert!(note.contains("format 5"), "{note}");
        assert!(note.contains("Upgrading"), "{note}");
        assert!(
            note.contains("already saved"),
            "the board's copy is banked — say so, that is what makes one click safe: {note}"
        );

        let too_old = RosterCardState::HoldsOldFormatProject {
            standing: DeviceFormatStanding::TooOld { found: Some(2) },
            expected: 5,
        };
        assert_eq!(too_old.status_line(0.0), "Holds a format-2 project");
        let note = too_old.sub_line(0.0).expect("a note");
        assert!(note.contains("too old"), "{note}");
        assert!(note.contains("wipe"), "the remedy is named: {note}");

        let newer = RosterCardState::HoldsOldFormatProject {
            standing: DeviceFormatStanding::FromNewerStudio { found: 9 },
            expected: 5,
        };
        assert_eq!(newer.status_line(0.0), "Holds a format-9 project");
        let note = newer.sub_line(0.0).expect("a note");
        assert!(
            note.contains("newer LightPlayer"),
            "this Studio is the one that is behind — say that: {note}"
        );

        // No version stamp at all: the headline must not invent one.
        let unstamped = RosterCardState::HoldsOldFormatProject {
            standing: DeviceFormatStanding::TooOld { found: None },
            expected: 5,
        };
        assert_eq!(
            unstamped.status_line(0.0),
            "Holds a project with no stated format"
        );
        assert!(unstamped.sub_line(0.0).is_some());
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
            "Push the latest"
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
