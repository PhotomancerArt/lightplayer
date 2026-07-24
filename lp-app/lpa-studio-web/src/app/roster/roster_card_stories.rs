//! The roster card vocabulary sheet: one story per direction.md state row,
//! plus the sim-card variant (D36), the standing firmware chip, and the
//! rich-object detail popover (the card's node-style detail trigger,
//! open).
//!
//! These stories are the visual-gate surface for the card grammar. Each
//! renders through the ONE shared card renderer
//! ([`DeviceCard`](crate::app::home::device_card::DeviceCard)) — the same
//! component the live gallery uses — fed by the core view-model
//! ([`RosterCardState`]), so the sheet can never drift from either the
//! vocabulary or the shipped card.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use lpa_studio_core::{
    BundledFirmware, ConnectPhase, DegradedReason, RosterCardState, UiDeviceCard,
    UiDeviceProjectChip,
};
use lpc_wire::FwProvenance;

use crate::app::home::device_card::DeviceCard;

/// A fixed "now" so the offline recency never drifts in baselines.
const STORY_NOW: f64 = 1_800_000_000.0;

#[story(description = "Green solid: running the local project's tip.")]
fn running_up_to_date() -> Element {
    sheet(vec![card(RosterCardState::RunningUpToDate, true)])
}

#[story(description = "Amber solid: running an older version; Push is the D11 consent.")]
fn running_behind() -> Element {
    sheet(vec![card(
        RosterCardState::RunningBehind {
            observed_version: Some(3),
            head_version: Some(5),
        },
        true,
    )])
}

#[story(description = "Amber solid: a genuine fork, already banked at connect (D8/D30).")]
fn edited_on_device() -> Element {
    sheet(vec![card(RosterCardState::EditedOnDevice, true)])
}

#[story(
    description = "Amber solid: crash recovery / safe mode (vocabulary slot — no live signal yet)."
)]
fn degraded() -> Element {
    sheet(vec![
        card(
            RosterCardState::Degraded {
                reason: DegradedReason::CrashRecovery,
            },
            true,
        ),
        card(
            RosterCardState::Degraded {
                reason: DegradedReason::SafeMode,
            },
            true,
        ),
    ])
}

#[story(description = "Amber pulsing: the connect retry ladder is working.")]
fn connecting_retrying() -> Element {
    sheet(vec![
        card(
            RosterCardState::ConnectingRetrying {
                phase: ConnectPhase::Connecting,
            },
            false,
        ),
        card(
            RosterCardState::ConnectingRetrying {
                phase: ConnectPhase::Resetting,
            },
            false,
        ),
    ])
}

#[story(description = "Amber pulsing: a long-running operation the user can walk away from.")]
fn operation_in_flight() -> Element {
    sheet(vec![card(
        RosterCardState::OperationInFlight {
            label: "Installing firmware".to_string(),
            percent: Some(62),
        },
        false,
    )])
}

#[story(description = "Green solid: live link, nothing loaded.")]
fn connected_empty() -> Element {
    sheet(vec![card(RosterCardState::ConnectedEmpty, false)])
}

#[story(
    description = "Amber solid: content Studio cannot read — detail as sub-line; replace or erase."
)]
fn holds_unreadable_data() -> Element {
    sheet(vec![card(
        RosterCardState::HoldsUnreadableData {
            detail: "project.json is not a current-format project".to_string(),
        },
        false,
    )])
}

#[story(description = "Amber solid: blank flash — provisioning turns it into a Device.")]
fn ready_to_set_up() -> Element {
    sheet(vec![card(RosterCardState::ReadyToSetUp, false)])
}

#[story(description = "Amber solid: recognized non-LightPlayer firmware, safe to replace.")]
fn other_firmware() -> Element {
    sheet(vec![card(RosterCardState::OtherFirmware, false)])
}

#[story(description = "Amber solid: wrong wire protocol — reflash is the only remedy.")]
fn needs_firmware_update() -> Element {
    sheet(vec![card(RosterCardState::NeedsFirmwareUpdate, false)])
}

#[story(description = "Amber solid: holds a project but no stamped identity.")]
fn needs_a_name() -> Element {
    sheet(vec![card(RosterCardState::NeedsAName, false)])
}

#[story(description = "Red solid: no classification within the deadline; troubleshoot.")]
fn not_responding() -> Element {
    sheet(vec![card(RosterCardState::NotResponding, false)])
}

#[story(description = "Gray solid: the port is held by another tab; quiet auto-retry.")]
fn in_use_elsewhere() -> Element {
    sheet(vec![card(RosterCardState::InUseElsewhere, false)])
}

#[story(description = "Gray hollow, faded: remembered only; click reconnects.")]
fn offline() -> Element {
    sheet(vec![card(
        RosterCardState::Offline {
            last_seen_at: Some(STORY_NOW - 2.0 * 86_400.0),
        },
        true,
    )])
}

#[story(description = "D36: same card grammar, sim glyph instead of the transport glyph.")]
fn simulator_runtime() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: device_card(RosterCardState::RunningUpToDate, true),
                now_secs: Some(STORY_NOW),
                sim: true,
                on_action: |_| {},
            }
        }
    }])
}

#[story(description = "The standing amber chip: firmware drift is advisory on any Running row.")]
fn firmware_update_chip() -> Element {
    // the chip rides only an honest comparison: clean builds, differing
    // commits (dirty or unknown on either side suppresses it) — the card
    // compares the bundled image against the card's hello provenance
    sheet(vec![
        rsx! {
            div { class: "tw:w-64",
                DeviceCard {
                    card: device_card_with_fw(RosterCardState::RunningUpToDate, true),
                    now_secs: Some(STORY_NOW),
                    bundled_fw: Some(bundled_firmware()),
                    on_action: |_| {},
                }
            }
        },
        // project drift owns the circle; the firmware chip stays advisory
        rsx! {
            div { class: "tw:w-64",
                DeviceCard {
                    card: device_card_with_fw(
                        RosterCardState::RunningBehind {
                            observed_version: Some(3),
                            head_version: Some(5),
                        },
                        true,
                    ),
                    now_secs: Some(STORY_NOW),
                    bundled_fw: Some(bundled_firmware()),
                    on_action: |_| {},
                }
            }
        },
    ])
}

#[story(
    description = "The rich-object detail popover on a live Running-behind device, open from the card's node-style trigger (Q1: the affordance-following icon on the right; the circle stays a pure indicator). Fixed schema order — Health, Project, Technical — with the danger zone pinned last as the inline red-tinted section (Q5): Flash firmware and Erase migrated here from the interim More-menu. The advisory firmware chip tones the Technical section, never the trigger."
)]
fn device_detail_running_behind() -> Element {
    rsx! {
        div { class: "tw:min-h-[640px] tw:p-4",
            div { class: "tw:w-64",
                DeviceCard {
                    card: device_card_with_fw(
                        RosterCardState::RunningBehind {
                            observed_version: Some(3),
                            head_version: Some(5),
                        },
                        true,
                    ),
                    now_secs: Some(STORY_NOW),
                    bundled_fw: Some(bundled_firmware()),
                    detail_open: true,
                    on_action: |_| {},
                }
            }
        }
    }
}

#[story(
    description = "The rich-object detail popover on an offline (remembered) device: quiet trigger, Neutral rollup; Health carries Reconnect, Project shows the last-ran copy, Technical keeps the registered identity, and the danger zone holds Forget (the offline card's old More-menu row)."
)]
fn device_detail_offline() -> Element {
    rsx! {
        div { class: "tw:min-h-[520px] tw:p-4",
            div { class: "tw:w-64",
                DeviceCard {
                    card: device_card(
                        RosterCardState::Offline {
                            last_seen_at: Some(STORY_NOW - 2.0 * 86_400.0),
                        },
                        true,
                    ),
                    now_secs: Some(STORY_NOW),
                    detail_open: true,
                    on_action: |_| {},
                }
            }
        }
    }
}

/// Lay story cards out on the sheet.
fn sheet(cards: Vec<Element>) -> Element {
    rsx! {
        div { class: "tw:flex tw:flex-wrap tw:items-start tw:gap-3 tw:p-4",
            for card in cards {
                {card}
            }
        }
    }
}

/// A device card with the story defaults; `with_project` adds the header
/// chip (identity — shown wherever the device honestly holds/held one).
fn card(state: RosterCardState, with_project: bool) -> Element {
    rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: device_card(state, with_project),
                now_secs: Some(STORY_NOW),
                on_action: |_| {},
            }
        }
    }
}

fn device_card(state: RosterCardState, with_project: bool) -> UiDeviceCard {
    UiDeviceCard {
        uid: Some("dev_7pQr5St89uVwXy2C".to_string()),
        name: "Luna's porch sign".to_string(),
        transport: "USB".to_string(),
        state,
        project: with_project.then(|| UiDeviceProjectChip {
            uid: "prj_3fKq8Zr21bTxYw0A".to_string(),
            name: "porch-sign".to_string(),
        }),
        fw: None,
    }
}

/// The same card carrying hello firmware provenance (live-link Technical
/// evidence for the popover and the chip comparison).
fn device_card_with_fw(state: RosterCardState, with_project: bool) -> UiDeviceCard {
    UiDeviceCard {
        fw: Some(FwProvenance {
            package: "fw-esp32".to_string(),
            commit: "def987654321".to_string(),
            dirty: false,
            profile: "release-esp32".to_string(),
        }),
        ..device_card(state, with_project)
    }
}

/// A bundled image on a different clean commit than the running firmware,
/// so the honest comparison offers the update chip.
fn bundled_firmware() -> BundledFirmware {
    BundledFirmware {
        commit: "abc123456789".to_string(),
        dirty: false,
    }
}
