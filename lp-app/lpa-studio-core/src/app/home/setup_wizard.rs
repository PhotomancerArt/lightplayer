//! The setup wizard's card: the P11 machine's state made renderable.
//!
//! Design: `docs/design/device-setup-flow.md`; UI ruling F5b (gallery
//! product vision, G1 round 2b) — **the wizard IS a card**, amended at the
//! G2 gate (2026-08-05) to say WHICH card: while nothing is attached it is
//! a standalone card in the entry-cards slot, and from the port grant
//! onward it is the **body of the bound device's own roster card**
//! ([`UiSetupWizard::takeover_card`]). The wizard is a state of the device
//! card, never a second card for one physical board.
//!
//! Nothing here decides anything. [`SetupSession`] is the controller's
//! hold on one running flow (the machine, the target it opened on, and the
//! session the executor addresses); [`UiSetupWizard`] is the snapshot the
//! renderer draws. Every transition still belongs to
//! [`SetupFlow`](crate::SetupFlow), and every effect to
//! [`dispatch_for`](crate::dispatch_for).
//!
//! Two things live here rather than in the renderer, because they are data
//! and not layout: the **steps rail** (which big step a machine state is
//! in) and the **project line** (the compact `meteor → 256-px strip → IO8`
//! the PROVISION step promises). A component that derived either would be
//! a component that knows the flow.

use lpa_boards::board_by_id;

use crate::app::setup_flow::{
    SetupCapabilities, SetupContext, SetupFlow, SetupGesture, SetupState, SetupStateKind,
};
use crate::core::log::UiLogEntry;

use super::board_project::DEFAULT_STRIP_PIXELS;
use super::card_ui_state::CardOp;

/// One rail step's position relative to where the flow stands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiSetupRailPhase {
    Done,
    Now,
    Todo,
}

/// One step on the wizard's rail (`Connect › Flash › Project › Done`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiSetupRailStep {
    pub label: String,
    pub phase: UiSetupRailPhase,
}

/// The PROVISION step's project box: one compact line and the ⓘ detail
/// behind it (spike §4 — card width settled this; the full node flow does
/// not fit and never needed to).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiSetupProject {
    /// The board's catalog `display_name` — "made for the …".
    pub board_label: String,
    /// `meteor → 256-px strip → IO8`.
    pub summary: String,
    /// What the compact line leaves out.
    pub detail: String,
}

impl UiSetupProject {
    /// The generated first project, described for `board_id`. `None` when
    /// the catalog does not know the board — the same refusal
    /// [`generate_board_project`](super::board_project::generate_board_project)
    /// makes, rather than a made-up pin.
    pub fn for_board(board_id: &str) -> Option<Self> {
        let board = board_by_id(board_id)?;
        let wire = board.default_led_wire()?;
        Some(Self {
            board_label: board.display_name.clone(),
            summary: format!("meteor → {DEFAULT_STRIP_PIXELS}-px strip → {wire}"),
            detail: format!(
                "Also: a clock, a one-entry playlist, and target: {board_id} recorded on the \
                 project (advisory — the engine never reads it)."
            ),
        })
    }
}

/// Everything the wizard card renders, for one machine state.
///
/// The renderer matches on [`Self::state`] — one component per state — and
/// reads the rest as trimmings. It never derives a transition.
#[derive(Clone, Debug, PartialEq)]
pub struct UiSetupWizard {
    /// Where the machine stands.
    pub state: SetupState,
    /// The target this flow opened on: a board on the wire, or THE
    /// simulator. Chrome only — the machine branches on capabilities.
    pub sim: bool,
    /// The card's title bar text.
    pub title: String,
    /// `Connect › Flash › Project › Done` (hardware) or `Board › Project ›
    /// Done` (sim).
    pub steps: Vec<UiSetupRailStep>,
    /// The live flash activity, straight off the card-owned op flow — the
    /// SAME value the device card's overlay renders, so the wizard's flash
    /// step cannot drift from the shipped one.
    pub flash: Option<CardOp>,
    /// The session's console tail, for the flash activity view's terminal.
    pub console_tail: Vec<UiLogEntry>,
    /// The generated project's compact line, at PROVISION.
    pub project: Option<UiSetupProject>,
    /// A generate-or-push failure. The machine has **no PROVISION failure
    /// edge** (design §7.10): rather than swallow the error or invent a
    /// transition, the wizard shows it and offers the ✕ it already has.
    pub error: Option<String>,
    /// The roster card this wizard rides as a **body takeover**, by
    /// [`UiDeviceCard::identity_key`](super::ui_device_card::UiDeviceCard::identity_key)
    /// — the G2 ruling (2026-08-05): the wizard is a STATE of the device
    /// card, not a card of its own. `None` means there is nothing to
    /// attach to yet (the pre-device states, and the whole sim path until
    /// the simulator starts), and the wizard renders standalone in the
    /// entry-cards slot.
    ///
    /// Resolved at view-build time against the assembled roster (the
    /// builder's `pin_setup_card`, which also pins that card first), so it
    /// follows the card's key as the uid lands mid-flow rather than
    /// pinning to a key that moves.
    pub takeover_card: Option<String>,
}

impl UiSetupWizard {
    /// The remembered name this board matched, when the probe recognised
    /// one — the "was Porch sign" copy on WLED_FOUND / BOARD_PICK /
    /// ALREADY_LP.
    pub fn recognised_name(&self) -> Option<&str> {
        self.state
            .probe()
            .and_then(|probe| probe.verdict.known())
            .map(|row| row.name.as_str())
            .filter(|name| !name.is_empty())
    }

    /// The chip one probe pass reported, for the board pick's filter and
    /// its "chip … detected" line.
    pub fn detected_chip(&self) -> Option<&str> {
        self.state
            .probe()
            .and_then(|probe| probe.detected_chip.as_deref())
    }

    /// Render as plain text for fallback renderers and tests.
    pub fn render_text_line(&self) -> String {
        format!(
            "Setup wizard [{}] {}",
            if self.sim { "sim" } else { "hardware" },
            self.state.kind().label()
        )
    }
}

/// The hardware rail: `Connect › Flash › Project › Done`.
const HARDWARE_RAIL: [(&str, &[SetupStateKind]); 4] = [
    (
        "Connect",
        &[
            SetupStateKind::ConnectIntro,
            SetupStateKind::BoardFirst,
            SetupStateKind::PortPicking,
        ],
    ),
    (
        "Flash",
        &[
            SetupStateKind::Probing,
            SetupStateKind::ProbeFailed,
            SetupStateKind::BoardPick,
            SetupStateKind::WledFound,
            SetupStateKind::AlreadyLp,
            SetupStateKind::Flashing,
            SetupStateKind::FlashFailed,
            SetupStateKind::AbandonGuard,
        ],
    ),
    ("Project", &[SetupStateKind::Provision]),
    ("Done", &[SetupStateKind::DeviceHome]),
];

/// The no-connect rail (design §7.1: BOARD_PICK is the sim's entry state).
const SIM_RAIL: [(&str, &[SetupStateKind]); 3] = [
    ("Board", &[SetupStateKind::BoardPick]),
    ("Project", &[SetupStateKind::Provision]),
    ("Done", &[SetupStateKind::DeviceHome]),
];

/// The rail for a state, on a target that does or does not need connecting.
pub fn setup_rail(needs_connect: bool, state: SetupStateKind) -> Vec<UiSetupRailStep> {
    let rail: &[(&str, &[SetupStateKind])] = if needs_connect {
        &HARDWARE_RAIL
    } else {
        &SIM_RAIL
    };
    // CLOSED belongs to no step; the card is gone by then anyway.
    let current = rail.iter().position(|(_, states)| states.contains(&state));
    rail.iter()
        .enumerate()
        .map(|(index, (label, _))| UiSetupRailStep {
            label: (*label).to_string(),
            phase: match current {
                Some(current) if index < current => UiSetupRailPhase::Done,
                Some(current) if index == current => UiSetupRailPhase::Now,
                _ => UiSetupRailPhase::Todo,
            },
        })
        .collect()
}

/// One running setup flow, as the controller holds it.
///
/// The controller owns exactly one at a time: the wizard is a card, and two
/// wizard cards would be two flows racing for one serial port.
#[derive(Debug, Clone, PartialEq)]
pub struct SetupSession {
    /// The machine.
    pub flow: SetupFlow,
    /// Which of the two entry cards opened this (chrome + the sim's
    /// no-name rule are the only things that read it).
    pub sim: bool,
    /// The card key the executor addresses ops with — `None` until a port
    /// grant produces a session. The simulator's is known from the start.
    pub card_key: Option<String>,
    /// The last generate/push failure (design §7.10 — see
    /// [`UiSetupWizard::error`]).
    pub error: Option<String>,
}

impl SetupSession {
    /// Open a flow on the hardware target: connect, flash, name.
    pub fn hardware(stamp: impl Into<String>, taken_names: Vec<String>) -> Self {
        Self::start(SetupCapabilities::HARDWARE, false, None, stamp, taken_names)
    }

    /// Open a flow on THE simulator: no connect, no flash, no name.
    pub fn simulator(
        card_key: impl Into<String>,
        stamp: impl Into<String>,
        taken_names: Vec<String>,
    ) -> Self {
        Self::start(
            SetupCapabilities::SIMULATOR,
            true,
            Some(card_key.into()),
            stamp,
            taken_names,
        )
    }

    fn start(
        capabilities: SetupCapabilities,
        sim: bool,
        card_key: Option<String>,
        stamp: impl Into<String>,
        taken_names: Vec<String>,
    ) -> Self {
        Self {
            flow: SetupFlow::start(
                SetupContext::new(capabilities, stamp).with_taken_names(taken_names),
            ),
            sim,
            card_key,
            error: None,
        }
    }

    pub fn state(&self) -> &SetupState {
        self.flow.state()
    }

    /// Whether this flow has ended — the card comes off the grid.
    pub fn is_closed(&self) -> bool {
        matches!(self.state().kind(), SetupStateKind::Closed)
    }

    /// Apply one gesture. Returns what the executor must run.
    pub fn handle(&mut self, gesture: SetupGesture) -> Vec<crate::SetupCommand> {
        // A new gesture supersedes the last failure's copy: the user has
        // moved on, and a stale error under a live step is a lie.
        self.error = None;
        self.flow.handle(gesture.into())
    }

    /// The card snapshot, given what only the controller knows: the live
    /// flash op, the session's console tail, and which roster card (if
    /// any) this flow has bound to — [`UiSetupWizard::takeover_card`].
    pub fn view(
        &self,
        takeover_card: Option<String>,
        flash: Option<CardOp>,
        console_tail: Vec<UiLogEntry>,
    ) -> UiSetupWizard {
        let state = self.state().clone();
        let project = match &state {
            SetupState::Provision(provision) => UiSetupProject::for_board(&provision.board_id),
            _ => None,
        };
        UiSetupWizard {
            steps: setup_rail(self.flow.context().capabilities.needs_connect, state.kind()),
            title: if self.sim {
                "Simulate a device".to_string()
            } else {
                "Connect a device".to_string()
            },
            sim: self.sim,
            flash,
            console_tail,
            project,
            error: self.error.clone(),
            takeover_card,
            state,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::setup_flow::{BoardPickState, ProvisionPhase, ProvisionState};

    const C6: &str = "espressif/esp32-c6-devkitc-1";
    const STAMP: &str = "2026-08-04-1421";

    #[test]
    fn the_hardware_rail_walks_connect_flash_project_done() {
        let rail = setup_rail(true, SetupStateKind::Flashing);
        let labels: Vec<&str> = rail.iter().map(|step| step.label.as_str()).collect();
        assert_eq!(labels, ["Connect", "Flash", "Project", "Done"]);
        let phases: Vec<UiSetupRailPhase> = rail.iter().map(|step| step.phase).collect();
        assert_eq!(
            phases,
            [
                UiSetupRailPhase::Done,
                UiSetupRailPhase::Now,
                UiSetupRailPhase::Todo,
                UiSetupRailPhase::Todo
            ]
        );
    }

    #[test]
    fn the_sim_rail_starts_at_board() {
        let rail = setup_rail(false, SetupStateKind::BoardPick);
        let labels: Vec<&str> = rail.iter().map(|step| step.label.as_str()).collect();
        assert_eq!(labels, ["Board", "Project", "Done"]);
        assert_eq!(rail[0].phase, UiSetupRailPhase::Now);
    }

    #[test]
    fn every_state_but_closed_has_a_rail_position() {
        for kind in SetupStateKind::ALL {
            if kind == SetupStateKind::Closed {
                continue;
            }
            let hardware = setup_rail(true, kind);
            assert!(
                hardware
                    .iter()
                    .any(|step| step.phase == UiSetupRailPhase::Now),
                "{kind:?} has no step on the hardware rail — a state with no \
                 rendering is a spec gap, not a UI decision"
            );
        }
    }

    #[test]
    fn the_project_line_is_the_compact_one_the_spike_settled() {
        let project = UiSetupProject::for_board(C6).expect("the catalog knows the C6 devkit");
        assert!(
            project.summary.starts_with("meteor → 256-px strip → "),
            "{}",
            project.summary
        );
        assert!(project.detail.contains("advisory"));
    }

    #[test]
    fn an_unknown_board_describes_no_project() {
        assert_eq!(UiSetupProject::for_board("nobody/nothing"), None);
    }

    #[test]
    fn the_simulator_opens_on_the_board_pick_and_the_hardware_path_on_connect() {
        let sim = SetupSession::simulator("runtime-sim", STAMP, Vec::new());
        assert_eq!(sim.state().kind(), SetupStateKind::BoardPick);
        assert_eq!(sim.view(None, None, Vec::new()).title, "Simulate a device");

        let hardware = SetupSession::hardware(STAMP, Vec::new());
        assert_eq!(hardware.state().kind(), SetupStateKind::ConnectIntro);
        assert_eq!(
            hardware.view(None, None, Vec::new()).title,
            "Connect a device"
        );
    }

    #[test]
    fn a_gesture_clears_the_last_failures_copy() {
        let mut session = SetupSession::simulator("runtime-sim", STAMP, Vec::new());
        session.error = Some("the push failed".to_string());
        session.handle(SetupGesture::BoardChosen {
            board_id: C6.to_string(),
        });
        assert_eq!(session.error, None);
    }

    #[test]
    fn the_view_carries_the_project_line_only_at_provision() {
        let mut session = SetupSession::simulator("runtime-sim", STAMP, Vec::new());
        assert_eq!(session.view(None, None, Vec::new()).project, None);
        session.flow = {
            let mut flow = session.flow.clone();
            flow.handle(crate::SetupEvent::BoardChosen {
                board_id: C6.to_string(),
            });
            flow.handle(crate::SetupEvent::Confirm);
            flow
        };
        assert!(matches!(session.state(), SetupState::Provision(_)));
        assert!(session.view(None, None, Vec::new()).project.is_some());
    }

    #[test]
    fn recognition_reads_the_probe_the_state_already_carries() {
        let wizard = UiSetupWizard {
            state: SetupState::BoardPick(BoardPickState::default()),
            sim: false,
            title: String::new(),
            steps: Vec::new(),
            flash: None,
            console_tail: Vec::new(),
            project: None,
            error: None,
            takeover_card: None,
        };
        assert_eq!(wizard.recognised_name(), None);

        let provision = SetupState::Provision(ProvisionState {
            board_id: C6.to_string(),
            probe: None,
            name: "Porch sign".to_string(),
            phase: ProvisionPhase::Editing,
            project_uid: None,
        });
        let wizard = UiSetupWizard {
            state: provision,
            ..wizard
        };
        assert_eq!(wizard.recognised_name(), None, "no probe, no recognition");
    }
}
