//! The roster card vocabulary sheet: one story per direction.md state row,
//! plus the sim-card variant (D36), the standing firmware chip, and the
//! control-panel tabs open (Project / Settings / Danger — the sections
//! the retired detail popover carried, now card-resident; M7′ D39–D41).
//!
//! These stories are the visual-gate surface for the card grammar. Each
//! renders through the ONE shared card renderer
//! ([`DeviceCard`](crate::app::home::device_card::DeviceCard)) — the same
//! component the live gallery uses — fed by the core view-model
//! ([`RosterCardState`]), so the sheet can never drift from either the
//! vocabulary or the shipped card. State reads off the tint LEFT EDGE —
//! filled = live, double/faded = remembered, pulsing = working (the
//! retired circle's shape grammar, re-homed) — plus the D40 title bar
//! (kind glyph · inline name · transport · the always-visible grow ⤢).
//! There is no status circle.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use lpa_studio_core::LpFeature;
use lpa_studio_core::{
    BootloaderEntryFlow, BundledFirmware, CardOp, CardSheet, CardUiState, CardVerb, ConnectPhase,
    DegradedReason, DeviceCardTab, DeviceFormatStanding, RosterCardState, UiDeviceCard,
    UiDeviceProjectChip, UiLogEntry, UiLogLevel, UiLogOrigin, UiLogSource,
};
use lpc_wire::{BuildFacts, HardwareFacts};

use crate::app::home::device_card::DeviceCard;

/// Story helper: a card view-state opened on a given tab / sheet (the
/// capture equivalent of the retired `initial_tab`/`initial_sheet` props —
/// stories now drive the SAME core state the live app does).
fn opened(tab: DeviceCardTab, sheet: Option<CardSheet>) -> CardUiState {
    CardUiState {
        tab,
        sheet,
        op: None,
        setup_board: None,
    }
}

/// A fixed "now" so the offline recency never drifts in baselines.
const STORY_NOW: f64 = 1_800_000_000.0;

#[story(
    description = "Green filled edge: running the local project's tip. The hero strip (gallery-rework P05, vision D12) is the card's default identity treatment now — the project's art under the title bar with its name pill bottom-left, replacing the small in-body chip."
)]
fn running_up_to_date() -> Element {
    sheet(vec![card(RosterCardState::RunningUpToDate, true)])
}

#[story(description = "Amber filled edge: running an older version; Push is the D11 consent.")]
fn running_behind() -> Element {
    sheet(vec![card(behind_state(), true)])
}

#[story(
    description = "Amber filled edge: a genuine fork, already banked at connect (D8). §3c-2: BOTH verbs ride the face — Use board copy (adopt = overwrite-with-history) and Keep both (fork) — plus the editor CTA; the sub-line speaks the drift times in plain words. No Review hop, no sheet, no Stay (walking away is staying)."
)]
fn edited_on_device() -> Element {
    sheet(vec![card(
        RosterCardState::EditedOnDevice {
            local_saved_at: Some(STORY_NOW - 240.0),
            pushed_at: Some(STORY_NOW - 7_200.0),
        },
        true,
    )])
}

#[story(
    description = "Amber filled edge: crash recovery / safe mode (vocabulary slot — no live signal yet)."
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

#[story(description = "Amber pulsing edge: the connect retry ladder is working.")]
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

#[story(description = "Amber pulsing edge: a long-running operation the user can walk away from.")]
fn operation_in_flight() -> Element {
    sheet(vec![card(
        RosterCardState::OperationInFlight {
            label: "Installing firmware".to_string(),
            percent: Some(62),
        },
        false,
    )])
}

#[story(
    description = "The in-card push (M5): the Running-behind card's Push button was the D11 consent; the push now runs with progress folded into the card's Operation-in-flight state — same lane as flash/erase, project chip retained, no dialog."
)]
fn operation_pushing() -> Element {
    sheet(vec![card(
        RosterCardState::OperationInFlight {
            label: "Pushing v5".to_string(),
            percent: None,
        },
        true,
    )])
}

#[story(
    description = "Green filled edge: live link, nothing loaded; Choose-a-project jumps to the Project-tab picker (M8′). No project means no hero strip (gallery-rework P05) — the body's own \"nothing loaded\" status line carries the empty case."
)]
fn connected_empty() -> Element {
    sheet(vec![card(RosterCardState::ConnectedEmpty, false)])
}

#[story(
    description = "The Project-tab PICKER on the Connected-empty card (M8′, contract §3): the library's projects as menu rows — one click pushes (the click is the D11 consent). No popup, no dialog; the tab exists only while the library has something to offer."
)]
fn project_picker_open() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: UiDeviceCard {
                    port_label: None,
                    session_key: None,
                    ui: opened(DeviceCardTab::Project, None),
                    ..device_card(RosterCardState::ConnectedEmpty, false)
                },
                now_secs: Some(STORY_NOW),
                project_choices: vec![
                    UiDeviceProjectChip {
                        uid: "prj_3fKq8Zr21bTxYw0A".to_string(),
                        name: "porch-sign".to_string(),
                    },
                    UiDeviceProjectChip {
                        uid: "prj_9dLm2Xw44cRvZq1B".to_string(),
                        name: "2026-07-18-bedroom-lamp".to_string(),
                    },
                ],
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "Amber filled edge: content Studio cannot read — detail as sub-line; replace or erase."
)]
fn holds_unreadable_data() -> Element {
    sheet(vec![card(
        RosterCardState::HoldsUnreadableData {
            detail: "project.json is not a current-format project".to_string(),
        },
        false,
    )])
}

#[story(
    description = "Amber filled edge: the board holds a project at a format this Studio does not use (P5, 2026-08-04). Left: a format the migration chain reaches — the card names what it found and offers ONE verb, Upgrade, which pulls, migrates in the LIBRARY and pushes the result (the device is never rewritten in place, D14); it dispatches without a confirm because the pre-upgrade copy is already banked. Right: below the upgrade floor — no automatic path exists, so there is no button that would only refuse; the note names the remedy and the way out stays the wipe. Both replace what used to be a Running card lying about a board whose firmware had refused to load the project."
)]
fn holds_old_format_project() -> Element {
    sheet(vec![
        card(
            RosterCardState::HoldsOldFormatProject {
                standing: DeviceFormatStanding::Upgradable { found: 4 },
                expected: 5,
            },
            false,
        ),
        card(
            RosterCardState::HoldsOldFormatProject {
                standing: DeviceFormatStanding::TooOld { found: Some(2) },
                expected: 5,
            },
            false,
        ),
    ])
}

#[story(
    description = "Amber filled edge: blank flash — the Status tab IS the setup form (state-flow model §1-A): a prefilled date-default name + ONE Install button, no confirm, no separate naming dialog. The name lands in the registry at first post-flash contact, under the uid the board's own silicon derives."
)]
fn ready_to_set_up() -> Element {
    sheet(vec![card(RosterCardState::ReadyToSetUp, false)])
}

#[story(description = "Amber filled edge: recognized non-LightPlayer firmware, safe to replace.")]
fn other_firmware() -> Element {
    sheet(vec![card(RosterCardState::OtherFirmware, false)])
}

/// A blank board whose boot banner named the chip, with a board picked.
fn setup_card(detected_chip: Option<&str>, setup_board: Option<&str>) -> Element {
    let mut fixture = device_card(RosterCardState::ReadyToSetUp, false);
    fixture.detected_chip = detected_chip.map(str::to_string);
    fixture.ui.setup_board = setup_board.map(str::to_string);
    rsx! {
        div { class: "tw:w-72",
            DeviceCard {
                card: fixture,
                now_secs: Some(STORY_NOW),
                on_action: |_| {},
            }
        }
    }
}

#[story(
    description = "Setup form with a detected C6 (boot-banner evidence): only the C6 boards are offered, because a board for another chip cannot be flashed onto this device — the guard refuses it. Other-chip boards are not offered at all (gate-1 sitting 2026-08-03); the generic fallback is the dashed cell."
)]
fn setup_board_picker_detected_c6() -> Element {
    sheet(vec![setup_card(Some("esp32c6"), None)])
}

#[story(
    description = "The same collapse from the other side: an S3 is attached, so the S3 boards lead and the C6/classic ones fold. Regression guard for the hardware walk of 2026-08-02 — serving three builds made every non-matching board visible on every card, and a C6 user was shown four boards they could not use."
)]
fn setup_board_picker_detected_s3() -> Element {
    sheet(vec![setup_card(Some("esp32s3"), None)])
}

#[story(
    description = "Setup form with a board picked (accent border, core-owned CardUiState::setup_board — survives tab switches). Install writes this board's runtime manifest to /hardware.json after the flash."
)]
fn setup_board_picker_selected() -> Element {
    sheet(vec![setup_card(
        Some("esp32c6"),
        Some("seeed/xiao-esp32-c6"),
    )])
}

#[story(
    description = "Setup form with NO chip evidence (device booted before Studio attached): nothing can be ruled out, so every provisionable board is offered flat — this is the one case where the other-chip collapse would hide the board the user actually needs."
)]
fn setup_board_picker_unknown_chip() -> Element {
    sheet(vec![setup_card(None, None)])
}

#[story(description = "Amber filled edge: wrong wire protocol — reflash is the only remedy.")]
fn needs_firmware_update() -> Element {
    sheet(vec![card(RosterCardState::NeedsFirmwareUpdate, false)])
}

#[story(
    description = "Amber filled edge: a live board with no name yet; the Name-it row (and the title-bar name) open the D41 naming sheet — card-anchored, never a dialog."
)]
fn needs_a_name() -> Element {
    sheet(vec![card(RosterCardState::NeedsAName, false)])
}

#[story(
    description = "Red filled edge: the connect ladder's honest ending (try → reset+retry → not responding; M6) — the Troubleshoot affordance opens the card-resident instructions sheet."
)]
fn not_responding() -> Element {
    sheet(vec![card(RosterCardState::NotResponding, false)])
}

#[story(
    description = "The troubleshooting sheet (M6, D41) on the Not-responding card: concrete basic instructions (cable, replug, hold BOOT) with Reconnect re-running the ladder and the recovery flash one row below."
)]
fn troubleshoot_sheet_open() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: UiDeviceCard {
                    port_label: None,
                    session_key: None,
                    ui: opened(DeviceCardTab::Status, Some(CardSheet::Troubleshoot)),
                    ..device_card(RosterCardState::NotResponding, false)
                },
                now_secs: Some(STORY_NOW),
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "Amber filled edge: the chip is sitting in ROM download mode. Split out of Ready-to-set-up 2026-07-31 after a bench report — the two were collapsed, so Studio detected download mode and then discarded the fact, showing the blank-board flow instead. The load-bearing difference: a device flashed from here does NOT boot the new firmware on its own; it has to be physically replugged."
)]
fn recovery_mode() -> Element {
    sheet(vec![card(RosterCardState::RecoveryMode, false)])
}

#[story(
    description = "Bootloader-entry, step 1 (M5): the ritual for a chip Studio KNOWS — the card's firmware provenance named it, so the steps are specific. Every sequence starts by unplugging: the boot strap is sampled at reset, so holding BOOT on a running board does nothing."
)]
fn bootloader_entry_instructing() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: UiDeviceCard {
                    port_label: None,
                    session_key: None,
                    ui: opened(
                        DeviceCardTab::Status,
                        Some(CardSheet::BootloaderEntry(BootloaderEntryFlow::start(Some(
                            "fw-esp32c6",
                        )))),
                    ),
                    ..device_card(RosterCardState::NotResponding, false)
                },
                now_secs: Some(STORY_NOW),
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "Bootloader-entry for an UNKNOWN device (M5): Studio never reached this board, so it cannot name the chip. Generic steps, hedged button name, and an explicit admission that they may not match — an unhedged wrong instruction reads as a dead device."
)]
fn bootloader_entry_generic() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: UiDeviceCard {
                    port_label: None,
                    session_key: None,
                    ui: opened(
                        DeviceCardTab::Status,
                        Some(CardSheet::BootloaderEntry(BootloaderEntryFlow::start(None))),
                    ),
                    ..device_card(RosterCardState::NotResponding, false)
                },
                now_secs: Some(STORY_NOW),
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "Bootloader-entry, waiting (M5): the user has done the steps. Nothing is probed in this state — the probe reboots the device, so it fires only on a re-enumeration, which the ritual's replug already provides."
)]
fn bootloader_entry_waiting() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: UiDeviceCard {
                    port_label: None,
                    session_key: None,
                    ui: opened(
                        DeviceCardTab::Status,
                        Some(CardSheet::BootloaderEntry(
                            BootloaderEntryFlow::start(Some("fw-esp32c6")).begin_waiting(),
                        )),
                    ),
                    ..device_card(RosterCardState::NotResponding, false)
                },
                now_secs: Some(STORY_NOW),
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "Bootloader-entry, CONFIRMED (M5): the payoff, and the reason this flow exists. Without it a failed attempt and a dead board look identical, so people repeat the wrong motion and conclude the device is bricked."
)]
fn bootloader_entry_confirmed() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: UiDeviceCard {
                    port_label: None,
                    session_key: None,
                    ui: opened(
                        DeviceCardTab::Status,
                        Some(CardSheet::BootloaderEntry(
                            BootloaderEntryFlow::start(Some("fw-esp32c6"))
                                .begin_waiting()
                                .on_probe_answered(Some("ESP32-C6".to_string())),
                        )),
                    ),
                    ..device_card(RosterCardState::NotResponding, false)
                },
                now_secs: Some(STORY_NOW),
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "Bootloader-entry, not-yet (M5): the probe went unanswered. Deliberately NOT 'your device is broken' — an app-mode device ignores the handshake too, so the honest reading is that the attempt did not land."
)]
fn bootloader_entry_not_yet() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: UiDeviceCard {
                    port_label: None,
                    session_key: None,
                    ui: opened(
                        DeviceCardTab::Status,
                        Some(CardSheet::BootloaderEntry(
                            BootloaderEntryFlow::start(Some("fw-esp32c6"))
                                .begin_waiting()
                                .on_probe_unanswered(),
                        )),
                    ),
                    ..device_card(RosterCardState::NotResponding, false)
                },
                now_secs: Some(STORY_NOW),
                on_action: |_| {},
            }
        }
    }])
}

#[story(description = "Gray filled edge: the port is held by another tab; quiet auto-retry.")]
fn in_use_elsewhere() -> Element {
    sheet(vec![card(RosterCardState::InUseElsewhere, false)])
}

#[story(
    description = "Gray remembered edge (double line, whole card faded): remembered only; Reconnect lives on the Status tab as the state-table affordance (the old click-to-reconnect is retired). The hero strip dims to match (gallery-rework P05) — last-known art, not current, per the project chip's identity-not-health contract; no live preview lease for an offline card."
)]
fn offline() -> Element {
    sheet(vec![card(offline_state(), true)])
}

#[story(
    description = "Gallery-rework P05 gate: the hero strip's three device-card states side by side — Running (live art + name pill), Offline (dimmed, last-known art — identity, not health), and Connected-empty (no project, so no strip; the status line's \"nothing loaded\" carries it)."
)]
fn hero_strip_states() -> Element {
    sheet(vec![
        card(RosterCardState::RunningUpToDate, true),
        card(offline_state(), true),
        card(RosterCardState::ConnectedEmpty, false),
    ])
}

#[story(
    description = "D36: the LIVE sim card (runtime-pool P4) — same card grammar, sim glyph in the title bar, Running with the loaded project's chip; the grow control (⤢) re-attaches the editor lens to the sim session. The sim wears the same hero strip (gallery-rework P05) — no special-casing in the renderer."
)]
fn simulator_runtime() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: sim_card(true),
                now_secs: Some(STORY_NOW),
                sim: true,
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "D36: the live sim card with nothing loaded — the session exists, no project has been pushed; the grow control renders disabled (always visible, never a dead click)."
)]
fn simulator_nothing_loaded() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: sim_card(false),
                now_secs: Some(STORY_NOW),
                sim: true,
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "The Project tab open on a live Running-behind device: the drift facts (running v3, your copy v5) as fact rows with the Attention badge dot on the tab. Opening in the editor stays the grow control — the tab renders no duplicate row."
)]
fn project_tab_running_behind() -> Element {
    sheet(vec![tabbed(behind_state(), true, DeviceCardTab::Project)])
}

#[story(
    description = "The Settings tab open on a live Running device: the Technical facts (uid, transport, firmware provenance) with the advisory firmware-update chip — the chip badges the Settings tab, never the Status tab or the edge tint."
)]
fn settings_tab_running() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: UiDeviceCard {
                    port_label: None,
                    session_key: None,
                    ui: opened(DeviceCardTab::Settings, None),
                    ..device_card_with_fw(RosterCardState::RunningUpToDate, true)
                },
                now_secs: Some(STORY_NOW),
                bundled_fw: Some(bundled_firmware()),
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "G1 question 3 — the Settings tab's capability lines are GAPS-ONLY. Left: an all-capable device, whose Technical section says nothing extra (no noise where there is no news). Right: the same device on a build with no fluid/radio runtime and no radio wired — three added lines naming exactly what is missing."
)]
fn settings_tab_capability_gaps() -> Element {
    let gapped: Vec<LpFeature> = all_capable_features()
        .into_iter()
        .filter(|feature| {
            !matches!(
                feature,
                LpFeature::NodeFluid | LpFeature::NodeRadio | LpFeature::SvcRadioEspnow
            )
        })
        .collect();
    sheet(vec![
        rsx! {
            div { class: "tw:w-64",
                DeviceCard {
                    card: UiDeviceCard {
                        port_label: None,
                        session_key: None,
                        ui: opened(DeviceCardTab::Settings, None),
                        ..device_card_with_fw(RosterCardState::RunningUpToDate, true)
                    },
                    now_secs: Some(STORY_NOW),
                    on_action: |_| {},
                }
            }
        },
        rsx! {
            div { class: "tw:w-64",
                DeviceCard {
                    card: UiDeviceCard {
                        port_label: None,
                        session_key: None,
                        ui: opened(DeviceCardTab::Settings, None),
                        ..device_card_with_capabilities(
                            RosterCardState::RunningUpToDate,
                            true,
                            gapped,
                            false,
                        )
                    },
                    now_secs: Some(STORY_NOW),
                    on_action: |_| {},
                }
            }
        },
    ])
}

#[story(
    description = "The Danger tab open on a live Running-behind device: Flash firmware and Erase as destructive menu rows (confirmation on the actions). The tab wears the error family when selected and never badges — Danger never shouts."
)]
fn danger_tab_running_behind() -> Element {
    sheet(vec![tabbed(behind_state(), true, DeviceCardTab::Danger)])
}

#[story(
    description = "The Danger tab open on an offline (remembered) device: Forget is the only row (D34 hygiene) — no flash or erase without a live manageable link."
)]
fn danger_tab_offline() -> Element {
    sheet(vec![tabbed(offline_state(), true, DeviceCardTab::Danger)])
}

#[story(
    description = "The Danger tab open on the live sim card: Stop simulator as the destructive menu row (runtime-pool P3's destroy op; confirmation states the honest cost)."
)]
fn danger_tab_simulator() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: UiDeviceCard {
                    port_label: None,
                    session_key: None,
                    ui: opened(DeviceCardTab::Danger, None),
                    ..sim_card(true)
                },
                now_secs: Some(STORY_NOW),
                sim: true,
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "The erase confirm as a card-resident sheet (D41): the card dims below the title bar (the name stays readable — you always know whose sheet this is), the tint edge stays visible, and Erase wears the error family. THE destructive-confirm pattern — the native confirm() is retired for card actions; Cancel or clicking the backdrop dismisses."
)]
fn erase_sheet_open() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: UiDeviceCard {
                    port_label: None,
                    session_key: None,
                    ui: opened(DeviceCardTab::Danger, Some(CardSheet::Confirm(CardVerb::Erase))),
                    ..device_card(behind_state(), true)
                },
                now_secs: Some(STORY_NOW),
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "The naming sheet (D41, spike round 3) on the Needs-a-name card: input + Enter-to-save; the name writes to the registry and returns the card to Status. Supersedes the title-bar form for the unnamed board — a named device still renames inline in the title bar."
)]
fn name_sheet_open() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: UiDeviceCard {
                    port_label: None,
                    session_key: None,
                    ui: opened(DeviceCardTab::Status, Some(CardSheet::Name)),
                    ..device_card(RosterCardState::NeedsAName, false)
                },
                now_secs: Some(STORY_NOW),
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "The stop-simulator confirm as a card-resident sheet (D41) on the live sim card — the same pattern as erase; the honest cost stays in the copy."
)]
fn stop_sim_sheet_open() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: UiDeviceCard {
                    port_label: None,
                    session_key: None,
                    ui: opened(DeviceCardTab::Danger, Some(CardSheet::Confirm(CardVerb::StopSim))),
                    ..sim_card(true)
                },
                now_secs: Some(STORY_NOW),
                sim: true,
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "The in-place op overlay (device-lifecycle P2): a firmware install takes over the card BODY where it runs — the tab row is covered and the body blurred behind it, the title bar spared — with a determinate bar and the session's console tail as an open technical terminal. Never an app-level modal."
)]
fn op_overlay_determinate() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: UiDeviceCard {
                    port_label: None,
                    session_key: None,
                    ui: CardUiState {
                        op: Some(CardOp::new("Installing firmware…", Some(62))),
                        ..CardUiState::default()
                    },
                    ..device_card_with_console(RosterCardState::RunningUpToDate, true)
                },
                now_secs: Some(STORY_NOW),
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "The op flow's AwaitingDevice phase (state-flow model §2 I2): the op's EXPECTED disconnect — the board is rebooting after a flash — and the overlay stays up with the reconnect narration. The session is gone; the card-owned flow isn't."
)]
fn op_overlay_awaiting_device() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: UiDeviceCard {
                    port_label: None,
                    session_key: None,
                    ui: CardUiState {
                        op: Some(CardOp::awaiting("Waiting for firmware boot")),
                        ..CardUiState::default()
                    },
                    ..device_card_with_console(RosterCardState::RunningUpToDate, true)
                },
                now_secs: Some(STORY_NOW),
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "The op flow's Failed phase (state-flow model §2 I4): the error in the terminal and ONE exit to the nearest stable state — no in-place Retry, no silent fallback, no refresh. \"Copy details\" puts the whole context (error, device state, chip, board choice, running build, console tail) on the clipboard for an agent or a bug report; it sits quieter than the exit, which stays the one way out."
)]
fn op_overlay_failed() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: UiDeviceCard {
                    port_label: None,
                    session_key: None,
                    ui: CardUiState {
                        op: Some(CardOp::failed(
                            "Flashing firmware failed",
                            "esptool: timed out waiting for packet header",
                            "Back to set up",
                        )),
                        ..CardUiState::default()
                    },
                    ..device_card_with_console(RosterCardState::RunningUpToDate, true)
                },
                now_secs: Some(STORY_NOW),
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "The in-place op overlay with no known percent (device-lifecycle P2): the bar sweeps indeterminately while an erase runs, technical details streaming below."
)]
fn op_overlay_indeterminate() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: UiDeviceCard {
                    port_label: None,
                    session_key: None,
                    ui: CardUiState {
                        op: Some(CardOp::new("Erasing device…", None)),
                        ..CardUiState::default()
                    },
                    ..device_card_with_console(RosterCardState::RunningUpToDate, true)
                },
                now_secs: Some(STORY_NOW),
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "The per-device console strip (D42, card mode): the session's newest line rides the card's bottom edge as an ambient one-liner; clicking it jumps to the Console tab. The strip hides while that tab is active."
)]
fn console_strip() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: device_card_with_console(RosterCardState::RunningUpToDate, true),
                now_secs: Some(STORY_NOW),
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "The Console tab open (D42, card mode): the session's tail read-only, severity as line tint (warn amber, error red), the strip hidden while the tab is active. Display only in P2 — level/filter controls come later."
)]
fn console_tab_open() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: UiDeviceCard {
                    port_label: None,
                    session_key: None,
                    ui: opened(DeviceCardTab::Console, None),
                    ..device_card_with_console(RosterCardState::RunningUpToDate, true)
                },
                now_secs: Some(STORY_NOW),
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "D43: the card GROWN into the editor's right-side pane — same component, tall column, icon tabs stay icons, body scrolls, ⇲ shrinks back to the gallery. The console is a permanent expanded bottom region (round 3.5): the Console tab and the strip are gone in pane mode."
)]
fn pane_grown_device() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:h-[560px] tw:w-[340px]",
            DeviceCard {
                card: device_card_with_console(behind_state(), true),
                now_secs: Some(STORY_NOW),
                pane: true,
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "D43: the live sim card grown into the editor pane — sim glyph, the honestly-applicable tabs (no Settings/Performance), the permanent console region at the bottom."
)]
fn pane_grown_sim() -> Element {
    sheet(vec![rsx! {
        div { class: "tw:h-[560px] tw:w-[340px]",
            DeviceCard {
                card: UiDeviceCard {
                    port_label: None,
                    session_key: None,
                    console_tail: device_card_with_console(RosterCardState::RunningUpToDate, true)
                        .console_tail,
                    ..sim_card(true)
                },
                now_secs: Some(STORY_NOW),
                sim: true,
                pane: true,
                on_action: |_| {},
            }
        }
    }])
}

#[story(
    description = "The standing amber chip: firmware drift is advisory on any Running row — it badges the Settings tab, never the edge tint (project drift owns the edge)."
)]
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
        // project drift owns the edge tint; the firmware chip stays advisory
        rsx! {
            div { class: "tw:w-64",
                DeviceCard {
                    card: device_card_with_fw(behind_state(), true),
                    now_secs: Some(STORY_NOW),
                    bundled_fw: Some(bundled_firmware()),
                    on_action: |_| {},
                }
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

/// A device card opened on a control-panel tab (the tabs-open stories).
fn tabbed(state: RosterCardState, with_project: bool, tab: DeviceCardTab) -> Element {
    rsx! {
        div { class: "tw:w-64",
            DeviceCard {
                card: UiDeviceCard {
                    port_label: None,
                    session_key: None,
                    ui: opened(tab, None),
                    ..device_card(state, with_project)
                },
                now_secs: Some(STORY_NOW),
                on_action: |_| {},
            }
        }
    }
}

/// The Running-behind fixture the drift stories share.
fn behind_state() -> RosterCardState {
    RosterCardState::RunningBehind {
        observed_version: Some(3),
        head_version: Some(5),
    }
}

/// The offline fixture: remembered two days ago against the fixed clock.
fn offline_state() -> RosterCardState {
    RosterCardState::Offline {
        last_seen_at: Some(STORY_NOW - 2.0 * 86_400.0),
    }
}

fn device_card(state: RosterCardState, with_project: bool) -> UiDeviceCard {
    UiDeviceCard {
        port_label: None,
        session_key: None,
        uid: Some("dev_7pQr5St89uVwXy2C".to_string()),
        name: "Luna's porch sign".to_string(),
        transport: "USB".to_string(),
        state,
        project: with_project.then(|| UiDeviceProjectChip {
            uid: "prj_3fKq8Zr21bTxYw0A".to_string(),
            name: "porch-sign".to_string(),
        }),
        fw: None,
        hardware: None,
        safe_clamp: None,
        sim: false,
        console_tail: Vec::new(),
        ui: Default::default(),
        detected_chip: None,
    }
}

/// The live sim card fixture (D36): the shape `home_view_builder`'s
/// `sim_card` produces — Running with the loaded project's chip, or
/// "nothing loaded".
fn sim_card(with_project: bool) -> UiDeviceCard {
    UiDeviceCard {
        port_label: None,
        session_key: None,
        uid: None,
        name: "Simulator".to_string(),
        transport: String::new(),
        state: if with_project {
            RosterCardState::RunningUpToDate
        } else {
            RosterCardState::ConnectedEmpty
        },
        project: with_project.then(|| UiDeviceProjectChip {
            uid: "prj_3fKq8Zr21bTxYw0A".to_string(),
            name: "porch-sign".to_string(),
        }),
        fw: None,
        hardware: None,
        safe_clamp: None,
        sim: true,
        console_tail: Vec::new(),
        ui: Default::default(),
        detected_chip: None,
    }
}

/// The same card carrying a fixed console tail (D42 fixtures): engine
/// frames with one warn and one error, timestamps pinned to the story
/// clock so baselines never drift.
fn device_card_with_console(state: RosterCardState, with_project: bool) -> UiDeviceCard {
    let line = |offset: f64, level: UiLogLevel, message: &str| {
        UiLogEntry::new(
            STORY_NOW + offset,
            level,
            UiLogSource::with_detail(UiLogOrigin::Device, "fw-esp32c6"),
            message,
        )
    };
    UiDeviceCard {
        port_label: None,
        session_key: None,
        console_tail: vec![
            line(0.0, UiLogLevel::Info, "engine: project loaded · 241 points"),
            line(1.0, UiLogLevel::Info, "engine: frame 41022 · 60fps"),
            line(
                2.0,
                UiLogLevel::Warn,
                "engine: frame budget exceeded (21ms)",
            ),
            line(
                3.0,
                UiLogLevel::Error,
                "shader: uniform 'rate' out of range",
            ),
            line(4.0, UiLogLevel::Info, "engine: frame 41142 · 60fps"),
        ],
        ..device_card(state, with_project)
    }
}

/// The same card carrying hello firmware provenance (live-link Technical
/// evidence for the Settings tab and the chip comparison).
fn device_card_with_fw(state: RosterCardState, with_project: bool) -> UiDeviceCard {
    device_card_with_capabilities(state, with_project, all_capable_features(), true)
}

/// Everything a normal C6 build carries — the "no gaps" side of the
/// gaps-only Technical presentation.
fn all_capable_features() -> Vec<LpFeature> {
    vec![
        LpFeature::NodeButton,
        LpFeature::NodeClock,
        LpFeature::NodeFluid,
        LpFeature::NodeFixture,
        LpFeature::NodePlaylist,
        LpFeature::NodeRadio,
        LpFeature::NodeShader,
        LpFeature::NodeTexture,
        LpFeature::SvcButton,
        LpFeature::SvcRadioEspnow,
        LpFeature::GfxLpvm,
    ]
}

/// The same card whose hello reported a specific build and hardware set.
fn device_card_with_capabilities(
    state: RosterCardState,
    with_project: bool,
    features: Vec<LpFeature>,
    radio: bool,
) -> UiDeviceCard {
    UiDeviceCard {
        port_label: None,
        session_key: None,
        fw: Some(BuildFacts {
            features,
            package: "fw-esp32c6".to_string(),
            commit: "def987654321".to_string(),
            dirty: false,
            profile: "release-esp32".to_string(),
        }),
        hardware: Some(HardwareFacts {
            radio,
            button: true,
            board_id: None,
            ..Default::default()
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
