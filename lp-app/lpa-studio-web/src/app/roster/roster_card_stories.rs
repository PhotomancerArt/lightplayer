//! The roster card vocabulary sheet: one story per direction.md state row,
//! plus the sim-card variant (D36) and the standing firmware chip.
//!
//! These stories are the visual-gate surface for the card grammar. Each
//! renders the state through the core view-model ([`RosterCardState`]) —
//! circle, status line, sub-line, and affordance identity all come from
//! core, so the sheet can never drift from the vocabulary. The card
//! chrome here is a preview: the full card anatomy lands in M3.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use lpa_studio_core::{
    ConnectPhase, DegradedReason, RosterCardState, RosterCircle, RosterCircleShape as CoreShape,
    UiStatus, UiStatusKind, firmware_update_available,
};
use lpc_wire::FwProvenance;

use crate::base::{StatusCircle, StatusCircleShape, StatusCircleTone, StudioIcon, StudioIconName};
use crate::core::StatusChip;

/// A fixed "now" so the offline recency never drifts in baselines.
const STORY_NOW: f64 = 1_800_000_000.0;

#[story(description = "Green solid: running the local project's tip.")]
fn running_up_to_date() -> Element {
    sheet(vec![card(RosterCardState::RunningUpToDate)])
}

#[story(description = "Amber solid: running an older version; Push is the D11 consent.")]
fn running_behind() -> Element {
    sheet(vec![card(RosterCardState::RunningBehind {
        observed_version: Some(3),
        head_version: Some(5),
    })])
}

#[story(description = "Amber solid: a genuine fork, already banked at connect (D8/D30).")]
fn edited_on_device() -> Element {
    sheet(vec![card(RosterCardState::EditedOnDevice)])
}

#[story(
    description = "Amber solid: crash recovery / safe mode (vocabulary slot — no live signal yet)."
)]
fn degraded() -> Element {
    sheet(vec![
        card(RosterCardState::Degraded {
            reason: DegradedReason::CrashRecovery,
        }),
        card(RosterCardState::Degraded {
            reason: DegradedReason::SafeMode,
        }),
    ])
}

#[story(description = "Amber pulsing: the connect retry ladder is working.")]
fn connecting_retrying() -> Element {
    sheet(vec![
        card(RosterCardState::ConnectingRetrying {
            phase: ConnectPhase::Connecting,
        }),
        card(RosterCardState::ConnectingRetrying {
            phase: ConnectPhase::Resetting,
        }),
    ])
}

#[story(description = "Amber pulsing: a long-running operation the user can walk away from.")]
fn operation_in_flight() -> Element {
    sheet(vec![card(RosterCardState::OperationInFlight {
        label: "Installing firmware".to_string(),
        percent: Some(62),
    })])
}

#[story(description = "Green solid: live link, nothing loaded.")]
fn connected_empty() -> Element {
    sheet(vec![card(RosterCardState::ConnectedEmpty)])
}

#[story(description = "Amber solid: blank flash — provisioning turns it into a Device.")]
fn ready_to_set_up() -> Element {
    sheet(vec![card(RosterCardState::ReadyToSetUp)])
}

#[story(description = "Amber solid: recognized non-LightPlayer firmware, safe to replace.")]
fn other_firmware() -> Element {
    sheet(vec![card(RosterCardState::OtherFirmware)])
}

#[story(description = "Amber solid: wrong wire protocol — reflash is the only remedy.")]
fn needs_firmware_update() -> Element {
    sheet(vec![card(RosterCardState::NeedsFirmwareUpdate)])
}

#[story(description = "Amber solid: holds a project but no stamped identity.")]
fn needs_a_name() -> Element {
    sheet(vec![card(RosterCardState::NeedsAName)])
}

#[story(description = "Red solid: no classification within the deadline; troubleshoot.")]
fn not_responding() -> Element {
    sheet(vec![card(RosterCardState::NotResponding)])
}

#[story(description = "Gray solid: the port is held by another tab; quiet auto-retry.")]
fn in_use_elsewhere() -> Element {
    sheet(vec![card(RosterCardState::InUseElsewhere)])
}

#[story(description = "Gray hollow, faded: remembered only; click reconnects.")]
fn offline() -> Element {
    sheet(vec![card(RosterCardState::Offline {
        last_seen_at: Some(STORY_NOW - 2.0 * 86_400.0),
    })])
}

#[story(description = "D36: same card grammar, sim glyph instead of the transport glyph.")]
fn simulator_runtime() -> Element {
    sheet(vec![rsx! {
        RosterStoryCard {
            state: RosterCardState::RunningUpToDate,
            name: "Porch sign".to_string(),
            sim: true,
            fw_update: false,
        }
    }])
}

#[story(description = "The standing amber chip: firmware drift is advisory on any Running row.")]
fn firmware_update_chip() -> Element {
    // the chip rides only an honest comparison: clean builds, differing
    // commits (dirty or unknown on either side suppresses it)
    let bundled_commit = "abc123456789";
    let device_fw = FwProvenance {
        package: "fw-esp32".to_string(),
        commit: "def987654321".to_string(),
        dirty: false,
        profile: "release-esp32".to_string(),
    };
    let offered = firmware_update_available(bundled_commit, false, &device_fw);
    sheet(vec![
        rsx! {
            RosterStoryCard {
                state: RosterCardState::RunningUpToDate,
                name: "Workbench ESP32".to_string(),
                sim: false,
                fw_update: offered,
            }
        },
        // project drift owns the circle; the firmware chip stays advisory
        rsx! {
            RosterStoryCard {
                state: RosterCardState::RunningBehind {
                    observed_version: Some(3),
                    head_version: Some(5),
                },
                name: "Workbench ESP32".to_string(),
                sim: false,
                fw_update: offered,
            }
        },
    ])
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

/// A device card with the story defaults.
fn card(state: RosterCardState) -> Element {
    rsx! {
        RosterStoryCard {
            state,
            name: "Luna's porch sign".to_string(),
            sim: false,
            fw_update: false,
        }
    }
}

/// The card-grammar preview: header row (status circle · transport/sim
/// glyph), device name, status line, ≤1 sub-line, ≤1 affordance, standing
/// chips. Everything shown is read off the core view-model.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn RosterStoryCard(state: RosterCardState, name: String, sim: bool, fw_update: bool) -> Element {
    let (shape, tone) = circle_props(state.circle());
    let status_line = state.status_line(STORY_NOW);
    let sub_line = state.sub_line();
    let affordance = state.affordance().map(|affordance| affordance.label());
    let (glyph, transport_label) = if sim {
        (StudioIconName::Simulator, "Simulator")
    } else {
        (StudioIconName::Usb, "USB")
    };
    let faded = matches!(state, RosterCardState::Offline { .. });
    let card_class = if faded {
        "tw:w-64 tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card tw:opacity-70"
    } else {
        "tw:w-64 tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card"
    };

    rsx! {
        article { class: card_class,
            header { class: "tw:flex tw:items-center tw:gap-2 tw:border-b tw:border-border tw:bg-terminal tw:px-3 tw:py-2",
                StatusCircle { shape, tone }
                span { class: "tw:inline-flex tw:items-center tw:text-muted-foreground",
                    StudioIcon { name: glyph, size: 14 }
                }
                span { class: "tw:text-[11px] tw:font-bold tw:uppercase tw:tracking-wide tw:text-muted-foreground",
                    "{transport_label}"
                }
            }
            div { class: "tw:grid tw:gap-1 tw:p-3",
                p { class: "tw:m-0 tw:truncate tw:text-sm tw:font-semibold tw:text-strong-foreground",
                    "{name}"
                }
                p { class: "tw:m-0 tw:truncate tw:text-xs tw:text-dim-foreground", "{status_line}" }
                if let Some(sub_line) = sub_line {
                    p { class: "tw:m-0 tw:truncate tw:text-xs tw:text-subtle-foreground", "{sub_line}" }
                }
                if fw_update {
                    div { class: "tw:mt-1",
                        StatusChip { status: UiStatus::warning("Firmware update available") }
                    }
                }
                if let Some(label) = affordance {
                    div { class: "tw:mt-1",
                        button {
                            class: "tw:cursor-pointer tw:rounded-md tw:border tw:border-border-strong tw:bg-transparent tw:px-2 tw:py-1 tw:text-xs tw:font-semibold tw:text-strong-foreground",
                            r#type: "button",
                            "{label}"
                        }
                    }
                }
            }
        }
    }
}

/// Core circle spec → base component props (the one consumer-side hop —
/// base primitives stay independent of `lpa-studio-core`).
fn circle_props(circle: RosterCircle) -> (StatusCircleShape, StatusCircleTone) {
    let shape = match circle.shape {
        CoreShape::Solid => StatusCircleShape::Solid,
        CoreShape::Hollow => StatusCircleShape::Hollow,
        CoreShape::Pulsing => StatusCircleShape::Pulsing,
    };
    let tone = match circle.tone {
        UiStatusKind::Neutral => StatusCircleTone::Neutral,
        UiStatusKind::Working => StatusCircleTone::Working,
        UiStatusKind::Good => StatusCircleTone::Good,
        UiStatusKind::Warning => StatusCircleTone::Warning,
        UiStatusKind::Error => StatusCircleTone::Error,
    };
    (shape, tone)
}
