//! Evidence → card state: the normative roster derivation.
//!
//! [`derive_roster_card_state`] is a PURE function of the evidence a card
//! has, whatever its source — a live [`DeviceSession`] today, the registry
//! for remembered devices, discovery in the future. No source is assumed:
//! every field of [`RosterEvidence`] is optional except the connect
//! narration, and absence is itself evidence (registry-only = offline).
//!
//! The mapping is normative (direction.md "Card grammar", 2026-07-16
//! device-UX session), in precedence order — the first row whose evidence
//! is present wins:
//!
//! | Evidence | State |
//! |---|---|
//! | [`ConnectEvidence::OperationInFlight`] | Operation in flight |
//! | [`ConnectEvidence::Connecting`] | Connecting/retrying |
//! | [`ConnectEvidence::PortHeldElsewhere`] | In use elsewhere |
//! | [`ConnectEvidence::Failed`] (ladder exhausted) | Not responding |
//! | [`DeviceState::Booting`] | Connecting/retrying |
//! | [`DeviceState::BlankFlash`] / [`DeviceState::Bootloader`] | Ready to set up |
//! | [`DeviceState::ForeignFirmware`] | Other firmware |
//! | [`DeviceState::Incompatible`] | Needs firmware update |
//! | [`DeviceState::Unresponsive`] | Not responding |
//! | [`DeviceState::Ready`] + [`DeviceContent::Known`] `AtHead` | Running, up to date |
//! | [`DeviceState::Ready`] + [`DeviceContent::Known`] `Behind` | Running, behind (version via `ProjectHistory::version_number`, supplied as evidence) |
//! | [`DeviceState::Ready`] + [`DeviceContent::Known`] `Diverged` | Edited on device |
//! | [`DeviceState::Ready`] + [`DeviceContent::Adopted`] | Running, up to date (adoption made the library head this content) |
//! | [`DeviceState::Ready`] + [`DeviceContent::Empty`] | Connected, empty |
//! | [`DeviceState::Ready`] + [`DeviceContent::PendingIdentity`] | Needs a name |
//! | [`DeviceState::Ready`] + [`DeviceContent::Unreadable`] | Connected, empty (nothing loadABLE; "Choose a project" replaces the garbage — the honest affordance) |
//! | [`DeviceState::Ready`], pull not yet classified | Connecting/retrying (the attach isn't done until the pull lands) |
//! | [`DeviceState::Gone`], or no link at all | Offline (last seen from the registry) |
//!
//! `Degraded` is never derived here: no substrate signal exists yet (Q7).
//! The variant and its story exist so the vocabulary is complete.
//!
//! [`DeviceSession`]: lpa_link::DeviceSession

use lpa_link::DeviceState;

use crate::app::places::{DeviceContent, RegisteredDevice};
use lpc_history::SyncRelation;

use super::roster_card_state::{ConnectPhase, RosterCardState};

/// Everything a roster card may know, from any source. Assemble it from
/// whatever is on hand: a live session contributes `link` + `content`,
/// the registry contributes `registry`, the connect flow narrates
/// `connect`. Missing evidence is honest evidence of absence.
#[derive(Clone, Debug, PartialEq)]
pub struct RosterEvidence<'a> {
    /// The live link's observable state, when a session exists.
    pub link: Option<&'a DeviceState>,
    /// Connect-as-pull classification, once the pull has landed.
    pub content: Option<&'a DeviceContent>,
    /// The device copy's version number on the project line, looked up at
    /// evidence-assembly time via `ProjectHistory::version_number` (the
    /// derivation stays pure — no history access here).
    pub observed_version: Option<usize>,
    /// The local head's version number, for the "Push vN" affordance.
    pub head_version: Option<usize>,
    /// The registry entry, when the device is remembered.
    pub registry: Option<&'a RegisteredDevice>,
    /// What the connect flow / management operation is doing right now.
    pub connect: ConnectEvidence,
}

/// The connect-flow and operation narration, as evidence. Deliberately
/// its own vocabulary (not [`ConnectFlowState`](crate::ConnectFlowState)):
/// the retry ladder (D31 replacement) and D32 auto-connect land in later
/// milestones and will narrate through this same seam.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ConnectEvidence {
    /// Nothing in flight.
    #[default]
    Idle,
    /// The retry ladder is working a rung.
    Connecting { phase: ConnectPhase },
    /// A long-running management operation (flash, erase, push) is in
    /// flight.
    OperationInFlight { label: String, percent: Option<u8> },
    /// The port is held by another tab/process (D32 soft failure).
    PortHeldElsewhere,
    /// The ladder exhausted its rungs: honest failure.
    Failed,
}

/// Derive the card state from evidence. Pure; total over every evidence
/// combination. The module doc is the normative mapping.
pub fn derive_roster_card_state(evidence: &RosterEvidence<'_>) -> RosterCardState {
    match &evidence.connect {
        ConnectEvidence::OperationInFlight { label, percent } => {
            return RosterCardState::OperationInFlight {
                label: label.clone(),
                percent: *percent,
            };
        }
        ConnectEvidence::Connecting { phase } => {
            return RosterCardState::ConnectingRetrying { phase: *phase };
        }
        ConnectEvidence::PortHeldElsewhere => return RosterCardState::InUseElsewhere,
        ConnectEvidence::Failed => return RosterCardState::NotResponding,
        ConnectEvidence::Idle => {}
    }

    match evidence.link {
        Some(DeviceState::Booting) => RosterCardState::ConnectingRetrying {
            phase: ConnectPhase::Connecting,
        },
        Some(DeviceState::BlankFlash | DeviceState::Bootloader) => RosterCardState::ReadyToSetUp,
        Some(DeviceState::ForeignFirmware) => RosterCardState::OtherFirmware,
        Some(DeviceState::Incompatible { .. }) => RosterCardState::NeedsFirmwareUpdate,
        Some(DeviceState::Unresponsive { .. }) => RosterCardState::NotResponding,
        Some(DeviceState::Ready { .. }) => running_state(evidence),
        Some(DeviceState::Gone) | None => RosterCardState::Offline {
            last_seen_at: evidence.registry.map(|entry| entry.last_seen_at),
        },
    }
}

/// The Ready sub-mapping: what the device's contents mean for the card.
fn running_state(evidence: &RosterEvidence<'_>) -> RosterCardState {
    match evidence.content {
        Some(DeviceContent::Known { relation, .. }) => match relation {
            SyncRelation::AtHead => RosterCardState::RunningUpToDate,
            SyncRelation::Behind => RosterCardState::RunningBehind {
                observed_version: evidence.observed_version,
                head_version: evidence.head_version,
            },
            SyncRelation::Diverged => RosterCardState::EditedOnDevice,
        },
        Some(DeviceContent::Adopted { .. }) => RosterCardState::RunningUpToDate,
        Some(DeviceContent::Empty) => RosterCardState::ConnectedEmpty,
        Some(DeviceContent::PendingIdentity { .. }) => RosterCardState::NeedsAName,
        Some(DeviceContent::Unreadable { .. }) => RosterCardState::ConnectedEmpty,
        // ready but the connect-as-pull hasn't classified yet: still
        // attaching, honestly
        None => RosterCardState::ConnectingRetrying {
            phase: ConnectPhase::Connecting,
        },
    }
}

#[cfg(test)]
mod tests {
    use lpa_link::IncompatibleReason;
    use lpc_history::ContentHash;
    use lpc_wire::{FwProvenance, ServerHello, WIRE_PROTO_VERSION};

    use super::*;

    #[test]
    fn blank_flash_maps_to_ready_to_set_up() {
        assert_eq!(
            derive(&evidence().with_link(&DeviceState::BlankFlash)),
            RosterCardState::ReadyToSetUp
        );
    }

    #[test]
    fn bootloader_maps_to_ready_to_set_up() {
        // ROM download mode is the no-firmware family: waiting to be
        // flashed IS "ready to set up"
        assert_eq!(
            derive(&evidence().with_link(&DeviceState::Bootloader)),
            RosterCardState::ReadyToSetUp
        );
    }

    #[test]
    fn foreign_firmware_maps_to_other_firmware() {
        assert_eq!(
            derive(&evidence().with_link(&DeviceState::ForeignFirmware)),
            RosterCardState::OtherFirmware
        );
    }

    #[test]
    fn incompatible_maps_to_needs_firmware_update() {
        let link = DeviceState::Incompatible {
            reason: IncompatibleReason::NoHello,
        };
        assert_eq!(
            derive(&evidence().with_link(&link)),
            RosterCardState::NeedsFirmwareUpdate
        );
    }

    #[test]
    fn unresponsive_maps_to_not_responding() {
        let link = DeviceState::Unresponsive {
            diagnosis: lpa_link::device_session::BootDiagnosis::NoSerialOutput,
        };
        assert_eq!(
            derive(&evidence().with_link(&link)),
            RosterCardState::NotResponding
        );
    }

    #[test]
    fn pending_identity_maps_to_needs_a_name() {
        let ready = ready_link();
        let content = DeviceContent::PendingIdentity {
            observed: ContentHash::of(b"x"),
        };
        assert_eq!(
            derive(&evidence().with_link(&ready).with_content(&content)),
            RosterCardState::NeedsAName
        );
    }

    #[test]
    fn known_at_head_maps_to_running_up_to_date() {
        let ready = ready_link();
        let content = known(SyncRelation::AtHead);
        assert_eq!(
            derive(&evidence().with_link(&ready).with_content(&content)),
            RosterCardState::RunningUpToDate
        );
    }

    #[test]
    fn known_behind_maps_to_running_behind_with_versions() {
        let ready = ready_link();
        let content = known(SyncRelation::Behind);
        let mut evidence = evidence().with_link(&ready).with_content(&content);
        evidence.observed_version = Some(3);
        evidence.head_version = Some(5);
        assert_eq!(
            derive(&evidence),
            RosterCardState::RunningBehind {
                observed_version: Some(3),
                head_version: Some(5),
            }
        );
    }

    #[test]
    fn known_diverged_maps_to_edited_on_device() {
        let ready = ready_link();
        let content = known(SyncRelation::Diverged);
        assert_eq!(
            derive(&evidence().with_link(&ready).with_content(&content)),
            RosterCardState::EditedOnDevice
        );
    }

    #[test]
    fn adopted_content_maps_to_running_up_to_date() {
        let ready = ready_link();
        let content = DeviceContent::Adopted {
            project_uid: "prj_zzzzzzzzzzzzzzzz".to_string(),
            slug: "wild-one".to_string(),
            observed: ContentHash::of(b"w"),
        };
        assert_eq!(
            derive(&evidence().with_link(&ready).with_content(&content)),
            RosterCardState::RunningUpToDate
        );
    }

    #[test]
    fn empty_content_maps_to_connected_empty() {
        let ready = ready_link();
        let content = DeviceContent::Empty;
        assert_eq!(
            derive(&evidence().with_link(&ready).with_content(&content)),
            RosterCardState::ConnectedEmpty
        );
    }

    #[test]
    fn unreadable_content_maps_to_connected_empty() {
        // nothing loadABLE; "Choose a project" replaces the garbage —
        // the honest affordance (see module doc)
        let ready = ready_link();
        let content = DeviceContent::Unreadable {
            detail: "manifest unparseable".to_string(),
        };
        assert_eq!(
            derive(&evidence().with_link(&ready).with_content(&content)),
            RosterCardState::ConnectedEmpty
        );
    }

    #[test]
    fn ready_before_the_pull_lands_is_still_connecting() {
        let ready = ready_link();
        assert_eq!(
            derive(&evidence().with_link(&ready)),
            RosterCardState::ConnectingRetrying {
                phase: ConnectPhase::Connecting,
            }
        );
    }

    #[test]
    fn booting_is_connecting() {
        assert_eq!(
            derive(&evidence().with_link(&DeviceState::Booting)),
            RosterCardState::ConnectingRetrying {
                phase: ConnectPhase::Connecting,
            }
        );
    }

    #[test]
    fn registry_only_maps_to_offline_with_last_seen() {
        let entry = registered();
        let mut evidence = evidence();
        evidence.registry = Some(&entry);
        assert_eq!(
            derive(&evidence),
            RosterCardState::Offline {
                last_seen_at: Some(50.0),
            }
        );
    }

    #[test]
    fn gone_link_falls_back_to_the_registry_sighting() {
        let entry = registered();
        let mut evidence = evidence().with_link(&DeviceState::Gone);
        evidence.registry = Some(&entry);
        assert_eq!(
            derive(&evidence),
            RosterCardState::Offline {
                last_seen_at: Some(50.0),
            }
        );
    }

    #[test]
    fn no_evidence_at_all_is_offline_never_seen() {
        assert_eq!(
            derive(&evidence()),
            RosterCardState::Offline { last_seen_at: None }
        );
    }

    #[test]
    fn port_held_elsewhere_maps_to_in_use_elsewhere() {
        let mut evidence = evidence();
        evidence.connect = ConnectEvidence::PortHeldElsewhere;
        assert_eq!(derive(&evidence), RosterCardState::InUseElsewhere);
    }

    #[test]
    fn exhausted_retry_ladder_maps_to_not_responding() {
        let mut evidence = evidence();
        evidence.connect = ConnectEvidence::Failed;
        assert_eq!(derive(&evidence), RosterCardState::NotResponding);
    }

    #[test]
    fn an_operation_in_flight_wins_over_everything() {
        // mid-flash the link may read BlankFlash; the operation owns the
        // card until it settles
        let mut evidence = evidence().with_link(&DeviceState::BlankFlash);
        evidence.connect = ConnectEvidence::OperationInFlight {
            label: "Installing firmware".to_string(),
            percent: Some(62),
        };
        assert_eq!(
            derive(&evidence),
            RosterCardState::OperationInFlight {
                label: "Installing firmware".to_string(),
                percent: Some(62),
            }
        );
    }

    #[test]
    fn connect_evidence_narrates_over_a_stale_link_state() {
        let mut evidence = evidence().with_link(&DeviceState::Gone);
        evidence.connect = ConnectEvidence::Connecting {
            phase: ConnectPhase::Resetting,
        };
        assert_eq!(
            derive(&evidence),
            RosterCardState::ConnectingRetrying {
                phase: ConnectPhase::Resetting,
            }
        );
    }

    fn derive(evidence: &RosterEvidence<'_>) -> RosterCardState {
        derive_roster_card_state(evidence)
    }

    fn evidence() -> RosterEvidence<'static> {
        RosterEvidence {
            link: None,
            content: None,
            observed_version: None,
            head_version: None,
            registry: None,
            connect: ConnectEvidence::Idle,
        }
    }

    fn ready_link() -> DeviceState {
        DeviceState::Ready {
            hello: ServerHello {
                proto: WIRE_PROTO_VERSION,
                fw: FwProvenance {
                    package: "fw-esp32".to_string(),
                    commit: "abc123456789".to_string(),
                    dirty: false,
                    profile: "release-esp32".to_string(),
                },
                device_uid: Some("dev_0000000000000001".to_string()),
            },
        }
    }

    fn known(relation: SyncRelation) -> DeviceContent {
        DeviceContent::Known {
            project_uid: "prj_0000000000000001".to_string(),
            slug: "porch-sign".to_string(),
            observed: ContentHash::of(b"v"),
            relation,
        }
    }

    fn registered() -> RegisteredDevice {
        RegisteredDevice {
            uid: "dev_0000000000000001".to_string(),
            name: "Porch sign".to_string(),
            transport: "USB".to_string(),
            last_seen_at: 50.0,
            association: None,
        }
    }

    impl<'a> RosterEvidence<'a> {
        fn with_link(mut self, link: &'a DeviceState) -> Self {
            self.link = Some(link);
            self
        }

        fn with_content(mut self, content: &'a DeviceContent) -> Self {
            self.content = Some(content);
            self
        }
    }
}
