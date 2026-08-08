//! The setup wizard's vocabulary sheet: one static frame per machine
//! state, in the frame that state actually renders in.
//!
//! Every frame is built from the SAME core value the live gallery passes
//! ([`UiSetupWizard`]) and drawn by the same component, so the sheet can
//! never drift from the shipped wizard. Design:
//! `docs/design/device-setup-flow.md` §2 — the state list here is that
//! table's left column.
//!
//! Two frames, per the G2 ruling (2026-08-05):
//!
//! * [`frame`] — the standalone card in the entry-cards slot, for the
//!   states with no device to attach to yet.
//! * [`takeover`] — the bound board's OWN roster card with the wizard as
//!   its body, for every state from the port grant onward. The header is
//!   the device's and grows real facts as they land; the body is the flow.

use dioxus::prelude::*;
use lpa_studio_web_story_macros::story;

use lpa_studio_core::app::places::RegisteredDevice;
use lpa_studio_core::{
    BoardPickState, BoardProbe, BoardVerdict, CardOp, ConnectHint, ProvisionPhase, ProvisionState,
    RosterCardState, SetupState, UiDeviceCard, UiLogEntry, UiLogLevel, UiLogOrigin, UiLogSource,
    UiSetupProject, UiSetupWizard, setup_rail,
};

use crate::app::home::device_card::{ConnectDeviceCard, DeviceCard};
use crate::app::home::setup_wizard::SetupWizardCard;

const C6: &str = "espressif/esp32-c6-devkitc-1";
const CHIP: &str = "esp32c6";
/// Fixed clock: the card's status line reads it.
const NOW: f64 = 1_800_000_000.0;
/// The bound session's pool identity — the thread the takeover rides.
const SESSION: &str = "runtime-1";

/// A remembered board — the "was Porch sign" recognition corpus.
fn remembered() -> RegisteredDevice {
    RegisteredDevice {
        uid: "dev000000daqf6dvvqz".to_string(),
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
        hardware_uid: Some("dev000000daqf6dvvqz".to_string()),
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

/// The STANDALONE frame: the wizard as its own card in the entry-cards
/// slot, at the grid's real width. The states before a port grant (and the
/// whole sim path up to the start) have nothing to attach to, so this is
/// where they draw.
fn frame(wizard: UiSetupWizard) -> Element {
    rsx! {
        section { class: "tw:p-4",
            div { class: "tw:grid tw:grid-cols-[minmax(320px,380px)] tw:gap-3.5 tw:[grid-auto-rows:minmax(300px,auto)]",
                SetupWizardCard { wizard, on_action: |_| {} }
            }
        }
    }
}

/// The TAKEOVER frame: the bound board's own roster card — the same card
/// the live session produces, in the same grid slot — with the wizard as
/// its body. Nothing appears or disappears when the flow hands back; only
/// the body swaps.
fn takeover(card: UiDeviceCard, mut wizard: UiSetupWizard) -> Element {
    wizard.takeover_card = Some(card.identity_key().to_string());
    rsx! {
        section { class: "tw:p-4",
            div { class: "tw:grid tw:grid-cols-[minmax(320px,380px)] tw:gap-3.5 tw:[grid-auto-rows:minmax(300px,auto)]",
                DeviceCard {
                    card,
                    setup: Some(wizard),
                    now_secs: Some(NOW),
                    on_action: |_| {},
                }
            }
        }
    }
}

/// The bound board's card as it stands for most of the flow: anonymous
/// (no identity stamped yet, so it keys by its session), named by the chip
/// the probe reported, transport known from the moment the port opened.
fn bound_card() -> UiDeviceCard {
    UiDeviceCard {
        uid: None,
        session_key: Some(SESSION.to_string()),
        name: "ESP32-C6".to_string(),
        transport: "USB".to_string(),
        state: RosterCardState::ConnectedEmpty,
        project: None,
        fw: None,
        hardware: None,
        detected_chip: Some(CHIP.to_string()),
        board_id: None,
        port_label: Some("ESP32 Serial (0x303a:0x1001) · port-2".to_string()),
        safe_clamp: None,
        sim: false,
        console_tail: Vec::new(),
        frame_preview: None,
        frame_age_secs: None,
        frame_fps: None,
        ui: Default::default(),
    }
}

/// The same card once identity has landed — a uid and a name in the
/// header, while the body carries on with the same step. The header grows
/// real facts as they are learned; it never becomes a different card.
fn named_card() -> UiDeviceCard {
    UiDeviceCard {
        uid: Some("dev000000daqf6dvvqz".to_string()),
        name: "Porch sign".to_string(),
        ..bound_card()
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
        takeover_card: None,
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
        takeover_card: None,
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
    description = "PROBING, the last STANDALONE frame: the port is granted but no verdict has landed, so the connection has no identity yet — and a board the registry already remembers would show its remembered card next to an un-mergeable anonymous one. So the pre-verdict window keeps the wizard standalone and stands the bound session's row down; the card the flow will ride is the one the verdict names. One spinner for one probe pass — chip identity and what is already on the board, decided together (design §4)."
)]
fn probing() -> Element {
    frame(hardware(SetupState::Probing {
        preseeded_board: None,
    }))
}

#[story(
    description = "BOARD_PICK on a blank board — and the first frame of the TAKEOVER: the verdict has landed, so the board is recognised (or honestly anonymous, as here) and the wizard becomes the body of that board's own roster card. One physical board, one card, from here to the end. The header is the device's; the body is the flow. Inside it: the SHIPPED setup-form picker, filtered to the detected chip plus Generic. The forward verb is not armed until something is picked — the machine records selection and confirmation separately (design §7.2)."
)]
fn board_pick_blank() -> Element {
    takeover(
        bound_card(),
        hardware(SetupState::BoardPick(BoardPickState {
            probe: Some(probe(BoardVerdict::Blank { known: None })),
            selected: Some(C6.to_string()),
            replaces_firmware: false,
        })),
    )
}

#[story(
    description = "BOARD_PICK carrying recognition: the probed MAC matched a remembered row, so the card says whose board this was before offering to flash it. Reached from WLED_FOUND's wipe or ALREADY_LP's fresh setup, which is why the replacement warning rides along."
)]
fn board_pick_recognised() -> Element {
    takeover(
        named_card(),
        hardware(SetupState::BoardPick(BoardPickState {
            probe: Some(probe(BoardVerdict::Blank {
                known: Some(remembered()),
            })),
            selected: None,
            replaces_firmware: true,
        })),
    )
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
    takeover(
        bound_card(),
        hardware(SetupState::WledFound {
            probe: probe(BoardVerdict::Wled {
                known: Some(remembered()),
            }),
        }),
    )
}

#[story(
    description = "ALREADY_LP: adopt is one click and writes nothing but a sighting — the board keeps its name, its project, and its history. The registry name leads, because recognition is the whole point of this state. Done does NOT navigate (G2 follow-up, 2026-08-05): adopting a board that was already glowing is not a setup, and being thrown into the editor read as one — the flow just ends, this card goes back to its own body, and the user stays on the gallery. \"Open in the editor →\" is the secondary for whoever wanted that, and it is exactly what Done used to do."
)]
fn already_lightplayer() -> Element {
    takeover(
        named_card(),
        hardware(SetupState::AlreadyLp {
            probe: probe(BoardVerdict::LightPlayer {
                known: Some(remembered()),
            }),
        }),
    )
}

#[story(
    description = "PROBE_FAILED: retry, driver help, and back — never a dead end. The BOOT-button hint is the one trick that most often turns this state into a board pick."
)]
fn probe_failed() -> Element {
    takeover(
        // Nothing identified itself, so the card's header falls all the
        // way back to "Connected device" — the port is the only fact.
        UiDeviceCard {
            name: "Connected device".to_string(),
            detected_chip: None,
            ..bound_card()
        },
        hardware(SetupState::ProbeFailed {
            probe: BoardProbe {
                verdict: BoardVerdict::Unresponsive { known: None },
                detected_chip: None,
                hardware_uid: None,
                hardware_origin: None,
            },
        }),
    )
}

#[story(
    description = "FLASHING: the card-owned op flow's own activity view, verbatim — same label, same bar, same terminal the device card shows, now IN the card whose board is being written. The op rides the card (that is where the flash's progress is patched), and the wizard body reads it, so the two can no longer disagree; the card's own overlay stands down rather than narrating it twice."
)]
fn flashing() -> Element {
    let op = CardOp::new("Flashing firmware", Some(42));
    let mut wizard = hardware(SetupState::Flashing {
        board_id: C6.to_string(),
        probe: Some(probe(BoardVerdict::Blank { known: None })),
        attempt: 1,
    });
    wizard.flash = Some(op.clone());
    let mut card = bound_card();
    card.ui.op = Some(op);
    card.console_tail = console();
    takeover(card, wizard)
}

#[story(
    description = "FLASHING on the second attempt, riding out the expected disconnect: the op flow's AwaitingDevice phase (indeterminate, overlay stays up). An interrupted write is never trusted, so the step number is stated."
)]
fn flashing_retry() -> Element {
    let op = CardOp::awaiting("Waiting for the board…");
    let mut wizard = hardware(SetupState::Flashing {
        board_id: C6.to_string(),
        probe: Some(probe(BoardVerdict::Blank { known: None })),
        attempt: 2,
    });
    wizard.flash = Some(op.clone());
    let mut card = bound_card();
    card.ui.op = Some(op);
    card.console_tail = console();
    takeover(card, wizard)
}

#[story(
    description = "FLASH_FAILED after a second attempt: retry re-runs FROM ERASE, and the replug guidance appears because a wedged serial state survives soft resets. Abandon is the same act as closing here — a part-written board must never be left un-marked."
)]
fn flash_failed() -> Element {
    let mut card = bound_card();
    card.console_tail = console();
    takeover(
        card,
        hardware(SetupState::FlashFailed {
            board_id: C6.to_string(),
            probe: Some(probe(BoardVerdict::Blank { known: None })),
            attempt: 2,
            detail: "write failed at 0x6a000 — the device reset mid-write".to_string(),
        }),
    )
}

#[story(
    description = "ABANDON_GUARD: ✕ during a flash opens the card-resident sheet over an operation that never actually paused (design §7.8). Pressing ✕ again is inert — the sheet IS the answer to it. In the takeover the ✕ sits on the steps rail rather than the title bar, which now belongs to the device; abandoning has to stay one click away, so it moves rather than disappearing."
)]
fn abandon_guard() -> Element {
    let op = CardOp::new("Flashing firmware", Some(42));
    let mut wizard = hardware(SetupState::AbandonGuard {
        board_id: C6.to_string(),
        probe: Some(probe(BoardVerdict::Blank { known: None })),
        attempt: 1,
    });
    wizard.flash = Some(op.clone());
    let mut card = bound_card();
    card.ui.op = Some(op);
    card.console_tail = console();
    takeover(card, wizard)
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
    takeover(bound_card(), wizard)
}

#[story(
    description = "PROVISION mid-flight: one click, one generate. Confirm, ProjectGenerated, and PushCompleted are each inert out of phase, so the field and the verb are locked while the work runs. The header has grown its real name by now (the registry row landed with the confirm) — the same card, wearing one more fact."
)]
fn provision_working() -> Element {
    let mut wizard = hardware(SetupState::Provision(ProvisionState {
        board_id: C6.to_string(),
        probe: Some(probe(BoardVerdict::Blank { known: None })),
        name: "ESP32-C6 DevKitC-1 · Aug 5".to_string(),
        phase: ProvisionPhase::Pushing,
        project_uid: Some("prj3fKq8Zr21bTxYw0AhVmDpe".to_string()),
    }));
    wizard.project = UiSetupProject::for_board(C6);
    takeover(named_card(), wizard)
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
        project_uid: Some("prj3fKq8Zr21bTxYw0AhVmDpe".to_string()),
    }));
    wizard.project = UiSetupProject::for_board(C6);
    wizard.error = Some("the device disconnected while the project was being written".to_string());
    takeover(named_card(), wizard)
}

#[story(
    description = "DEVICE_HOME, drawn for completeness — the gallery no longer paints it anywhere. Under the G2 ruling the handoff is a BODY SWAP: at DEVICE_HOME the takeover simply ends and the bound card renders its own body again, in the same slot, so there is no landing frame to show and nothing appears or disappears. The copy is kept because the machine's state is real (the editor is already lensed to the target, vision D17); if the celebration is wanted back, it belongs in the card's own body, not in a card of the flow's."
)]
fn device_home() -> Element {
    frame(hardware(SetupState::DeviceHome {
        project_uid: Some("prj3fKq8Zr21bTxYw0AhVmDpe".to_string()),
        adopted: false,
    }))
}

#[story(
    description = "DEVICE_HOME reached by an adopt that asked for the editor (\"Open in the editor →\" — the only adopt edge that navigates since the G2 follow-up): it never stopped glowing, and nothing was written. Drawn for completeness like the frame above; plain Done ends at CLOSED(Adopted) instead, with the board's own card standing exactly where it already was."
)]
fn device_home_adopted() -> Element {
    frame(hardware(SetupState::DeviceHome {
        project_uid: None,
        adopted: true,
    }))
}

#[story(
    description = "DEVICE_HOME on the simulator path: the same landing, reached with no cable and no flash — and, like the two above, no longer painted: by the time the sim reaches it the sim card is on the grid and the project is open."
)]
fn device_home_simulator() -> Element {
    frame(simulated(SetupState::DeviceHome {
        project_uid: Some("prj3fKq8Zr21bTxYw0AhVmDpe".to_string()),
        adopted: false,
    }))
}

#[story(
    description = "CLOSED: the terminal state, drawn for completeness. In the live gallery the card is off the grid by the time this could paint. Four reasons reach it — Cancelled (✕ before anything was written), IncompleteFlash (shown here: the board's card is left saying it needs re-flashing), Adopted (Done on an already-LightPlayer board), and LeftConnected (✕ at PROVISION, after the flash landed). Only the first two release the port: a board that is flashed or adopted has earned its place on the roster, and dropping its session on the way out is how it ends up reading \"not connected\" one frame after being set up."
)]
fn closed() -> Element {
    frame(hardware(SetupState::Closed {
        reason: lpa_studio_core::CloseReason::IncompleteFlash,
    }))
}
