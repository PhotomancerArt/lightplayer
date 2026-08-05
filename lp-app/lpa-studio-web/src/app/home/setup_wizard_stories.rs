//! The setup wizard's vocabulary sheet: one static frame per machine
//! state, at the card width the flow actually renders at.
//!
//! Every frame is built from the SAME core value the live gallery passes
//! ([`UiSetupWizard`]) and drawn by the same component, so the sheet can
//! never drift from the shipped wizard. Design:
//! `docs/design/device-setup-flow.md` §2 — the state list here is that
//! table's left column.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use lpa_studio_core::app::places::RegisteredDevice;
use lpa_studio_core::{
    BoardPickState, BoardProbe, BoardVerdict, CardOp, ConnectHint, ProvisionPhase, ProvisionState,
    SetupState, UiLogEntry, UiLogLevel, UiLogOrigin, UiLogSource, UiSetupProject, UiSetupWizard,
    setup_rail,
};

use crate::app::home::device_card::ConnectDeviceCard;
use crate::app::home::setup_wizard::SetupWizardCard;

const C6: &str = "espressif/esp32-c6-devkitc-1";
const CHIP: &str = "esp32c6";

/// A remembered board — the "was Porch sign" recognition corpus.
fn remembered() -> RegisteredDevice {
    RegisteredDevice {
        uid: "dev_000000029EVDlKLX".to_string(),
        name: "Porch sign".to_string(),
        transport: "USB".to_string(),
        last_seen_at: 1_800_000_000.0,
        association: None,
        board_id: Some(C6.to_string()),
        hardware_id: Some("efuse:aa:bb:cc:dd:ee:ff".to_string()),
        previous_uids: Vec::new(),
    }
}

fn probe(verdict: BoardVerdict) -> BoardProbe {
    BoardProbe {
        verdict,
        detected_chip: Some(CHIP.to_string()),
        hardware_uid: Some("dev_000000029EVDlKLX".to_string()),
        hardware_origin: Some("efuse:aa:bb:cc:dd:ee:ff".to_string()),
    }
}

fn console() -> Vec<UiLogEntry> {
    [
        "Connected to ESP32-C6 bootloader",
        "Erasing flash",
        "Writing app image at 0x10000",
    ]
    .iter()
    .enumerate()
    .map(|(index, message)| {
        UiLogEntry::new(
            1_800_000_000.0 + index as f64,
            UiLogLevel::Info,
            UiLogSource::with_detail(UiLogOrigin::Device, "lpa-link"),
            *message,
        )
    })
    .collect()
}

/// The card the live grid renders, at its real width.
fn frame(wizard: UiSetupWizard) -> Element {
    rsx! {
        section { class: "tw:p-4",
            div { class: "tw:grid tw:grid-cols-[minmax(320px,380px)] tw:gap-3.5 tw:[grid-auto-rows:minmax(300px,auto)]",
                SetupWizardCard { wizard, on_action: |_| {} }
            }
        }
    }
}

/// A hardware-path wizard on `state`.
fn hardware(state: SetupState) -> UiSetupWizard {
    UiSetupWizard {
        steps: setup_rail(true, state.kind()),
        state,
        sim: false,
        title: "Connect a device".to_string(),
        flash: None,
        console_tail: Vec::new(),
        project: None,
        error: None,
    }
}

/// A simulator-path wizard on `state` (no connect, no flash, no name).
fn simulated(state: SetupState) -> UiSetupWizard {
    UiSetupWizard {
        steps: setup_rail(false, state.kind()),
        state,
        sim: true,
        title: "Simulate a device".to_string(),
        flash: None,
        console_tail: Vec::new(),
        project: None,
        error: None,
    }
}

#[story(
    description = "The two entry cards, half height in one grid cell: connect a device (a board on the end of a cable) and simulate a device (the sim, which takes on a real board). Both open the same machine — the difference is the target's capabilities, never a branch."
)]
fn entry_cards() -> Element {
    rsx! {
        section { class: "tw:p-4",
            div { class: "tw:grid tw:grid-cols-[minmax(320px,380px)] tw:gap-3.5 tw:[grid-auto-rows:minmax(300px,auto)]",
                ConnectDeviceCard { on_action: |_| {} }
            }
        }
    }
}

#[story(
    description = "CONNECT_INTRO, first arrival: two stacked full-width CTAs. The primary opens the browser's own port chooser through the machine's RequestPort; the secondary starts from the board, which is where driver help lives."
)]
fn connect_intro() -> Element {
    frame(hardware(SetupState::ConnectIntro {
        hint: ConnectHint::None,
    }))
}

#[story(
    description = "CONNECT_INTRO after the chooser came back empty: the hint escalates toward the secondary CTA, because a board that never appears in the list usually needs a driver rather than another click."
)]
fn connect_intro_no_ports() -> Element {
    frame(hardware(SetupState::ConnectIntro {
        hint: ConnectHint::NoPortsSeen,
    }))
}

#[story(
    description = "CONNECT_INTRO after the port went away mid-flow — the disconnected hint. Every hardware state that holds a port lands here (design §2 cross-cutting), so the copy has to be about the cable, not about the step that was interrupted."
)]
fn connect_intro_disconnected() -> Element {
    frame(hardware(SetupState::ConnectIntro {
        hint: ConnectHint::Disconnected,
    }))
}

#[story(
    description = "BOARD_FIRST with a CH340-class board picked: the full catalog through the shipped picker, then board-specific connect guidance. This state absorbs the driver help — the board that looks dead on macOS is the case it exists for."
)]
fn board_first() -> Element {
    frame(hardware(SetupState::BoardFirst {
        chosen: Some("quinled/dig-uno".to_string()),
    }))
}

#[story(
    description = "PORT_PICKING: the browser owns the dialog, so the card only says what is happening and waits."
)]
fn port_picking() -> Element {
    frame(hardware(SetupState::PortPicking {
        preseeded_board: None,
    }))
}

#[story(
    description = "PROBING: one spinner for one probe pass — chip identity and what is already on the board, decided together (design §4)."
)]
fn probing() -> Element {
    frame(hardware(SetupState::Probing {
        preseeded_board: None,
    }))
}

#[story(
    description = "BOARD_PICK on a blank board: the SHIPPED setup-form picker, filtered to the detected chip plus Generic. The forward verb is not armed until something is picked — the machine records selection and confirmation separately (design §7.2)."
)]
fn board_pick_blank() -> Element {
    frame(hardware(SetupState::BoardPick(BoardPickState {
        probe: Some(probe(BoardVerdict::Blank { known: None })),
        selected: Some(C6.to_string()),
        replaces_firmware: false,
    })))
}

#[story(
    description = "BOARD_PICK carrying recognition: the probed MAC matched a remembered row, so the card says whose board this was before offering to flash it. Reached from WLED_FOUND's wipe or ALREADY_LP's fresh setup, which is why the replacement warning rides along."
)]
fn board_pick_recognised() -> Element {
    frame(hardware(SetupState::BoardPick(BoardPickState {
        probe: Some(probe(BoardVerdict::Blank {
            known: Some(remembered()),
        })),
        selected: None,
        replaces_firmware: true,
    })))
}

#[story(
    description = "BOARD_PICK as the simulator's ENTRY state (design §7.1 — there is no separate SIM_BOARD_PICK): the full catalog, no chip filter, no Generic cell, and Continue instead of Flash because the target needs no flash."
)]
fn board_pick_simulator() -> Element {
    frame(simulated(SetupState::BoardPick(BoardPickState {
        probe: None,
        selected: Some(C6.to_string()),
        replaces_firmware: false,
    })))
}

#[story(
    description = "WLED_FOUND: the wipe offer, with the migration promise deliberately absent. Presets stay in WLED's own backups; today is wipe-and-set-up and the copy says so."
)]
fn wled_found() -> Element {
    frame(hardware(SetupState::WledFound {
        probe: probe(BoardVerdict::Wled {
            known: Some(remembered()),
        }),
    }))
}

#[story(
    description = "ALREADY_LP: adopt is one click and writes nothing but a sighting — the board keeps its name, its project, and its history. The registry name leads, because recognition is the whole point of this state."
)]
fn already_lightplayer() -> Element {
    frame(hardware(SetupState::AlreadyLp {
        probe: probe(BoardVerdict::LightPlayer {
            known: Some(remembered()),
        }),
    }))
}

#[story(
    description = "PROBE_FAILED: retry, driver help, and back — never a dead end. The BOOT-button hint is the one trick that most often turns this state into a board pick."
)]
fn probe_failed() -> Element {
    frame(hardware(SetupState::ProbeFailed {
        probe: BoardProbe {
            verdict: BoardVerdict::Unresponsive { known: None },
            detected_chip: None,
            hardware_uid: None,
            hardware_origin: None,
        },
    }))
}

#[story(
    description = "FLASHING: the card-owned op flow's own activity view, verbatim — same label, same bar, same terminal the device card shows. The wizard runs the existing provisioning op; it does not have a second flash path."
)]
fn flashing() -> Element {
    let mut wizard = hardware(SetupState::Flashing {
        board_id: C6.to_string(),
        probe: Some(probe(BoardVerdict::Blank { known: None })),
        attempt: 1,
    });
    wizard.flash = Some(CardOp::new("Flashing firmware", Some(42)));
    wizard.console_tail = console();
    frame(wizard)
}

#[story(
    description = "FLASHING on the second attempt, riding out the expected disconnect: the op flow's AwaitingDevice phase (indeterminate, overlay stays up). An interrupted write is never trusted, so the step number is stated."
)]
fn flashing_retry() -> Element {
    let mut wizard = hardware(SetupState::Flashing {
        board_id: C6.to_string(),
        probe: Some(probe(BoardVerdict::Blank { known: None })),
        attempt: 2,
    });
    wizard.flash = Some(CardOp::awaiting("Waiting for the board…"));
    wizard.console_tail = console();
    frame(wizard)
}

#[story(
    description = "FLASH_FAILED after a second attempt: retry re-runs FROM ERASE, and the replug guidance appears because a wedged serial state survives soft resets. Abandon is the same act as closing here — a part-written board must never be left un-marked."
)]
fn flash_failed() -> Element {
    frame(hardware(SetupState::FlashFailed {
        board_id: C6.to_string(),
        probe: Some(probe(BoardVerdict::Blank { known: None })),
        attempt: 2,
        detail: "write failed at 0x6a000 — the device reset mid-write".to_string(),
    }))
}

#[story(
    description = "ABANDON_GUARD: ✕ during a flash opens the card-resident sheet over an operation that never actually paused (design §7.8). Pressing ✕ again is inert — the sheet IS the answer to it."
)]
fn abandon_guard() -> Element {
    let mut wizard = hardware(SetupState::AbandonGuard {
        board_id: C6.to_string(),
        probe: Some(probe(BoardVerdict::Blank { known: None })),
        attempt: 1,
    });
    wizard.flash = Some(CardOp::new("Flashing firmware", Some(42)));
    wizard.console_tail = console();
    frame(wizard)
}

#[story(
    description = "PROVISION on hardware: new project only — the compact line the card width settled on (spike §4), with the clock, playlist, and advisory target behind ⓘ — plus the derived device name. Derived, rarely typed: this board has been seen before, so it keeps the name it already had."
)]
fn provision_hardware() -> Element {
    let mut wizard = hardware(SetupState::Provision(ProvisionState {
        board_id: C6.to_string(),
        probe: Some(probe(BoardVerdict::Blank {
            known: Some(remembered()),
        })),
        name: "Porch sign".to_string(),
        phase: ProvisionPhase::Editing,
        project_uid: None,
    }));
    wizard.project = UiSetupProject::for_board(C6);
    frame(wizard)
}

#[story(
    description = "PROVISION mid-flight: one click, one generate. Confirm, ProjectGenerated, and PushCompleted are each inert out of phase, so the field and the verb are locked while the work runs."
)]
fn provision_working() -> Element {
    let mut wizard = hardware(SetupState::Provision(ProvisionState {
        board_id: C6.to_string(),
        probe: Some(probe(BoardVerdict::Blank { known: None })),
        name: "ESP32-C6 DevKitC-1 · Aug 5".to_string(),
        phase: ProvisionPhase::Pushing,
        project_uid: Some("prj_3fKq8Zr21bTxYw0AhVmDpe".to_string()),
    }));
    wizard.project = UiSetupProject::for_board(C6);
    frame(wizard)
}

#[story(
    description = "PROVISION on the simulator: the same step with no name field at all — the target cannot be renamed, so the single sim stays \"Simulator\" rather than growing a name nobody asked for."
)]
fn provision_simulator() -> Element {
    let mut wizard = simulated(SetupState::Provision(ProvisionState {
        board_id: C6.to_string(),
        probe: None,
        name: String::new(),
        phase: ProvisionPhase::Editing,
        project_uid: None,
    }));
    wizard.project = UiSetupProject::for_board(C6);
    frame(wizard)
}

#[story(
    description = "PROVISION after a generate or push that did not land. The machine has NO failure edge here (design §7.10) — what a failed push leaves on a flashed, registered board is undecided — so the wizard reports it plainly and leaves the ✕ as the door, rather than inventing a transition."
)]
fn provision_failed() -> Element {
    let mut wizard = hardware(SetupState::Provision(ProvisionState {
        board_id: C6.to_string(),
        probe: Some(probe(BoardVerdict::Blank { known: None })),
        name: "ESP32-C6 DevKitC-1 · Aug 5".to_string(),
        phase: ProvisionPhase::Pushing,
        project_uid: Some("prj_3fKq8Zr21bTxYw0AhVmDpe".to_string()),
    }));
    wizard.project = UiSetupProject::for_board(C6);
    wizard.error = Some("the device disconnected while the project was being written".to_string());
    frame(wizard)
}

#[story(
    description = "DEVICE_HOME, the handoff frame: the flow has landed and the editor is already lensed to the target (vision D17). The card exists only until the real device card replaces it."
)]
fn device_home() -> Element {
    frame(hardware(SetupState::DeviceHome {
        project_uid: Some("prj_3fKq8Zr21bTxYw0AhVmDpe".to_string()),
        adopted: false,
    }))
}

#[story(
    description = "DEVICE_HOME reached by adopting an already-LightPlayer board: it never stopped glowing, and nothing was written."
)]
fn device_home_adopted() -> Element {
    frame(hardware(SetupState::DeviceHome {
        project_uid: None,
        adopted: true,
    }))
}

#[story(
    description = "DEVICE_HOME on the simulator path: the same landing, reached with no cable and no flash."
)]
fn device_home_simulator() -> Element {
    frame(simulated(SetupState::DeviceHome {
        project_uid: Some("prj_3fKq8Zr21bTxYw0AhVmDpe".to_string()),
        adopted: false,
    }))
}

#[story(
    description = "CLOSED: the terminal state, drawn for completeness. In the live gallery the card is off the grid by the time this could paint."
)]
fn closed() -> Element {
    frame(hardware(SetupState::Closed {
        reason: lpa_studio_core::CloseReason::IncompleteFlash,
    }))
}
