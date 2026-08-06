//! R1 — the setup flow as a pure reducer.
//!
//! Design: `docs/design/device-setup-flow.md` §2. **The doc's transition
//! table and the match arms below are one artifact**; [`tests`] walks the
//! full `(State, Event)` product so a pair nobody thought about is visible
//! rather than merely absent.
//!
//! No I/O, no async, no clock, no UI types. The only outside data the
//! reducer reads is the embedded board catalog (`lpa_boards`), which is a
//! static table, and the injected library stamp.

use lpa_boards::board_by_id;

use super::command::SetupCommand;
use super::event::SetupEvent;
use super::naming::derive_device_name;
use super::state::{
    BoardPickState, CloseReason, ConnectHint, ProvisionPhase, ProvisionState, SetupState,
};
use super::target::{SetupCapabilities, SetupTarget};
use super::verdict::{BoardProbe, BoardVerdict};

/// What the reducer knows that is not the state: the target's capabilities
/// and the two facts naming needs. All injected — core reads no clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupContext {
    pub capabilities: SetupCapabilities,
    /// The library's injected local stamp, `YYYY-MM-DD-HHMM`.
    pub stamp: String,
    /// Every remembered device's name, for collision suffixes. The row this
    /// board itself matched is filtered out where it matters.
    pub taken_names: Vec<String>,
}

impl SetupContext {
    pub fn new(capabilities: SetupCapabilities, stamp: impl Into<String>) -> Self {
        Self {
            capabilities,
            stamp: stamp.into(),
            taken_names: Vec::new(),
        }
    }

    pub fn with_taken_names(mut self, names: Vec<String>) -> Self {
        self.taken_names = names;
        self
    }
}

/// One reduction: where the flow lands and what it asks for.
#[derive(Debug, Clone, PartialEq)]
pub struct SetupStep {
    pub state: SetupState,
    pub commands: Vec<SetupCommand>,
}

impl SetupStep {
    fn go(state: SetupState) -> Self {
        Self {
            state,
            commands: Vec::new(),
        }
    }

    fn with(state: SetupState, commands: Vec<SetupCommand>) -> Self {
        Self { state, commands }
    }
}

/// The flow: a context, a position, and nothing else.
#[derive(Debug, Clone, PartialEq)]
pub struct SetupFlow {
    context: SetupContext,
    state: SetupState,
}

impl SetupFlow {
    /// Open the flow on a target. A target that needs connecting starts at
    /// CONNECT_INTRO; one that does not starts at the board pick with no
    /// probe evidence (design §7.1 — this is what used to be a separate
    /// `SIM_BOARD_PICK`).
    pub fn start(context: SetupContext) -> Self {
        let state = if context.capabilities.needs_connect {
            SetupState::ConnectIntro {
                hint: ConnectHint::None,
            }
        } else {
            SetupState::BoardPick(BoardPickState::default())
        };
        Self { context, state }
    }

    /// Open the flow on a [`SetupTarget`].
    pub fn for_target(
        target: &dyn SetupTarget,
        stamp: impl Into<String>,
        taken_names: Vec<String>,
    ) -> Self {
        Self::start(SetupContext::new(target.capabilities(), stamp).with_taken_names(taken_names))
    }

    pub fn state(&self) -> &SetupState {
        &self.state
    }

    pub fn context(&self) -> &SetupContext {
        &self.context
    }

    /// Apply one event, returning the commands it asks for.
    pub fn handle(&mut self, event: SetupEvent) -> Vec<SetupCommand> {
        let state = core::mem::replace(
            &mut self.state,
            SetupState::Closed {
                reason: CloseReason::Cancelled,
            },
        );
        let step = reduce(&self.context, state, event);
        self.state = step.state;
        step.commands
    }
}

/// `(State, Event) → (State, Vec<Command>)`.
///
/// A pair with no arm below is **inert**: the state comes back unchanged
/// and nothing is asked for. That is the deliberate answer for a gesture
/// that cannot happen in a state (a stale click, a late event from a
/// superseded step) — the alternative, panicking on the user, is worse.
/// The transition tests enumerate the whole product so inertness is
/// asserted rather than assumed.
pub fn reduce(context: &SetupContext, state: SetupState, event: SetupEvent) -> SetupStep {
    let caps = context.capabilities;
    match (state, event) {
        // ---- CONNECT_INTRO -------------------------------------------------
        (SetupState::ConnectIntro { .. }, SetupEvent::ItsConnected) => SetupStep::with(
            SetupState::PortPicking {
                preseeded_board: None,
            },
            vec![SetupCommand::RequestPort],
        ),
        (SetupState::ConnectIntro { .. }, SetupEvent::PickBoardFirst) => {
            SetupStep::go(SetupState::BoardFirst { chosen: None })
        }

        // ---- BOARD_FIRST ---------------------------------------------------
        (SetupState::BoardFirst { .. }, SetupEvent::BoardChosen { board_id }) => {
            SetupStep::go(SetupState::BoardFirst {
                chosen: Some(board_id),
            })
        }
        (SetupState::BoardFirst { chosen }, SetupEvent::ItsPluggedIn) => SetupStep::with(
            SetupState::PortPicking {
                preseeded_board: chosen,
            },
            vec![SetupCommand::RequestPort],
        ),
        (SetupState::BoardFirst { .. }, SetupEvent::Back) => SetupStep::go(connect_intro()),

        // ---- PORT_PICKING --------------------------------------------------
        (SetupState::PortPicking { preseeded_board }, SetupEvent::PortGranted) => SetupStep::with(
            SetupState::Probing { preseeded_board },
            vec![SetupCommand::ProbeBoard],
        ),
        (SetupState::PortPicking { .. }, SetupEvent::PortPickerCancelled) => {
            SetupStep::go(SetupState::ConnectIntro {
                hint: ConnectHint::PickerClosed,
            })
        }
        (SetupState::PortPicking { .. }, SetupEvent::PortPickerEmpty) => {
            SetupStep::go(SetupState::ConnectIntro {
                hint: ConnectHint::NoPortsSeen,
            })
        }

        // ---- PROBING -------------------------------------------------------
        (SetupState::Probing { preseeded_board }, SetupEvent::ProbeCompleted { probe }) => {
            SetupStep::go(route_probe(probe, preseeded_board))
        }

        // ---- BOARD_PICK ----------------------------------------------------
        (SetupState::BoardPick(pick), SetupEvent::BoardChosen { board_id }) => {
            SetupStep::go(SetupState::BoardPick(BoardPickState {
                selected: Some(board_id),
                ..pick
            }))
        }
        (SetupState::BoardPick(pick), SetupEvent::Confirm) => match pick.selected.clone() {
            // Nothing picked yet: the forward verb is not armed.
            None => SetupStep::go(SetupState::BoardPick(pick)),
            Some(board_id) if caps.needs_flash => {
                let attempt = 1;
                SetupStep::with(
                    SetupState::Flashing {
                        board_id: board_id.clone(),
                        probe: pick.probe,
                        attempt,
                    },
                    vec![SetupCommand::Flash {
                        board_id,
                        attempt,
                        replug_guidance: false,
                    }],
                )
            }
            Some(board_id) => SetupStep::go(enter_provision(context, board_id, pick.probe)),
        },
        // The board question got answered somewhere else while the picker
        // was still asking it (G1b ruling 6): an "Open in sim" landed a
        // project on the simulator, and the project's advisory `target`
        // came with it. The flow is DONE rather than short-cut — its
        // PROVISION step generates a starter project and pushes it, which
        // here would overwrite the project the user just opened, while
        // everything setup exists to produce is already true.
        //
        // Two ways this stays put, and neither is a kind check (R2):
        // a target that still `needs_flash` is not set up by a project
        // landing elsewhere — it has no firmware yet — and a board that
        // could not be inferred leaves the picker the only way to answer.
        (SetupState::BoardPick(pick), SetupEvent::SetUpElsewhere { board_id }) => {
            match board_id {
                Some(_) if !caps.needs_flash => SetupStep::go(closed(CloseReason::SetUpElsewhere)),
                _ => SetupStep::go(SetupState::BoardPick(pick)),
            }
        }
        (SetupState::BoardPick(_), SetupEvent::Back) => SetupStep::with(
            // On a no-connect target BOARD_PICK is the entry state, so
            // "back" leaves the flow (design §7.1).
            if caps.needs_connect {
                connect_intro()
            } else {
                closed(CloseReason::Cancelled)
            },
            release_port_if(caps.needs_connect),
        ),

        // ---- WLED_FOUND ----------------------------------------------------
        (SetupState::WledFound { probe }, SetupEvent::WipeAndSetUp) => {
            SetupStep::go(SetupState::BoardPick(BoardPickState {
                probe: Some(probe),
                selected: None,
                replaces_firmware: true,
            }))
        }
        (SetupState::WledFound { .. }, SetupEvent::Back) => {
            SetupStep::with(connect_intro(), vec![SetupCommand::ReleasePort])
        }

        // ---- ALREADY_LP ----------------------------------------------------
        // Adopt writes nothing but the sighting: the board keeps its name,
        // its project, and its history. The two adopt edges differ ONLY in
        // where the user is left (G2 follow-up, 2026-08-05 — adopt does
        // not navigate, setup does): "Done" ends the flow where the user
        // already is, on the gallery, with the board on its own card;
        // "Open in the editor" is the old landing, kept as the secondary.
        (SetupState::AlreadyLp { probe }, SetupEvent::AdoptDone) => SetupStep::with(
            // NO ReleasePort: adopting means the board joins the roster as
            // it is, and it cannot do that with its session dropped.
            closed(CloseReason::Adopted),
            record_sighting_for(&probe),
        ),
        (SetupState::AlreadyLp { probe }, SetupEvent::AdoptAndOpen) => {
            let mut commands = record_sighting_for(&probe);
            commands.push(SetupCommand::OpenDeviceHome);
            SetupStep::with(
                SetupState::DeviceHome {
                    project_uid: None,
                    adopted: true,
                },
                commands,
            )
        }
        (SetupState::AlreadyLp { probe }, SetupEvent::SetUpFresh) => {
            SetupStep::go(SetupState::BoardPick(BoardPickState {
                probe: Some(probe),
                selected: None,
                replaces_firmware: true,
            }))
        }

        // ---- PROBE_FAILED --------------------------------------------------
        (SetupState::ProbeFailed { .. }, SetupEvent::Retry) => SetupStep::with(
            SetupState::Probing {
                preseeded_board: None,
            },
            vec![SetupCommand::ProbeBoard],
        ),
        (SetupState::ProbeFailed { .. }, SetupEvent::PickBoardFirst) => SetupStep::with(
            SetupState::BoardFirst { chosen: None },
            vec![SetupCommand::ReleasePort],
        ),
        (SetupState::ProbeFailed { .. }, SetupEvent::Back) => {
            SetupStep::with(connect_intro(), vec![SetupCommand::ReleasePort])
        }

        // ---- FLASHING ------------------------------------------------------
        (
            SetupState::Flashing {
                board_id, probe, ..
            },
            SetupEvent::FlashSucceeded,
        ) => SetupStep::go(enter_provision(context, board_id, probe)),
        (
            SetupState::Flashing {
                board_id,
                probe,
                attempt,
            },
            SetupEvent::FlashFailed { detail },
        ) => SetupStep::go(SetupState::FlashFailed {
            board_id,
            probe,
            attempt,
            detail,
        }),
        (
            SetupState::Flashing {
                board_id,
                probe,
                attempt,
            },
            SetupEvent::PortLost,
        ) => SetupStep::go(SetupState::FlashFailed {
            board_id,
            probe,
            attempt,
            detail: PORT_LOST_DURING_FLASH.to_string(),
        }),
        (
            SetupState::Flashing {
                board_id,
                probe,
                attempt,
            },
            SetupEvent::CloseRequested,
        ) => SetupStep::go(SetupState::AbandonGuard {
            board_id,
            probe,
            attempt,
        }),

        // ---- FLASH_FAILED --------------------------------------------------
        (
            SetupState::FlashFailed {
                board_id,
                probe,
                attempt,
                ..
            },
            SetupEvent::Retry,
        ) => {
            let attempt = attempt.saturating_add(1);
            SetupStep::with(
                SetupState::Flashing {
                    board_id: board_id.clone(),
                    probe,
                    attempt,
                },
                vec![SetupCommand::Flash {
                    board_id,
                    attempt,
                    // From the second attempt on, a physical replug is
                    // usually the thing that unsticks an ESP32.
                    replug_guidance: attempt >= 2,
                }],
            )
        }
        // Abandoning a part-written flash — or closing on it, which is the
        // same act — leaves a board that must never be trusted to boot.
        (SetupState::FlashFailed { .. }, SetupEvent::Abandon | SetupEvent::CloseRequested) => {
            SetupStep::with(
                closed(CloseReason::IncompleteFlash),
                vec![SetupCommand::MarkIncompleteFlash, SetupCommand::ReleasePort],
            )
        }

        // ---- ABANDON_GUARD -------------------------------------------------
        (
            SetupState::AbandonGuard {
                board_id,
                probe,
                attempt,
            },
            SetupEvent::KeepFlashing,
        ) => SetupStep::go(SetupState::Flashing {
            board_id,
            probe,
            attempt,
        }),
        (SetupState::AbandonGuard { .. }, SetupEvent::Abandon) => SetupStep::with(
            closed(CloseReason::IncompleteFlash),
            vec![SetupCommand::MarkIncompleteFlash, SetupCommand::ReleasePort],
        ),
        // The flash never actually paused, so it can land — or fail —
        // while the guard sheet is open (design §7.8).
        (
            SetupState::AbandonGuard {
                board_id, probe, ..
            },
            SetupEvent::FlashSucceeded,
        ) => SetupStep::go(enter_provision(context, board_id, probe)),
        (
            SetupState::AbandonGuard {
                board_id,
                probe,
                attempt,
            },
            SetupEvent::FlashFailed { detail },
        ) => SetupStep::go(SetupState::FlashFailed {
            board_id,
            probe,
            attempt,
            detail,
        }),
        (
            SetupState::AbandonGuard {
                board_id,
                probe,
                attempt,
            },
            SetupEvent::PortLost,
        ) => SetupStep::go(SetupState::FlashFailed {
            board_id,
            probe,
            attempt,
            detail: PORT_LOST_DURING_FLASH.to_string(),
        }),

        // ---- PROVISION -----------------------------------------------------
        (SetupState::Provision(provision), SetupEvent::NameEdited { name }) => {
            if !caps.can_rename {
                return SetupStep::go(SetupState::Provision(provision));
            }
            SetupStep::go(SetupState::Provision(ProvisionState { name, ..provision }))
        }
        (SetupState::Provision(provision), SetupEvent::Confirm) => {
            if provision.phase != ProvisionPhase::Editing {
                // Already working: a second click must not generate twice.
                return SetupStep::go(SetupState::Provision(provision));
            }
            let board_id = provision.board_id.clone();
            SetupStep::with(
                SetupState::Provision(ProvisionState {
                    phase: ProvisionPhase::Generating,
                    ..provision
                }),
                vec![SetupCommand::GenerateProject { board_id }],
            )
        }
        (SetupState::Provision(provision), SetupEvent::ProjectGenerated { project_uid }) => {
            if provision.phase != ProvisionPhase::Generating {
                return SetupStep::go(SetupState::Provision(provision));
            }
            let mut commands = Vec::new();
            // The registry write is the whole of provisioning's persistence:
            // name and board under the device's identity. No stamp step —
            // identity is anchored in silicon and survives an erase.
            //
            // The probe's uid is ADVISORY, not a gate (G2 blank-C6 walk):
            // a blank board anchors no identity while it sits in its boot
            // loop, and the flash we just ran is what gives it one. Gating
            // the write on the PROBE's uid meant the name the user typed
            // was written nowhere, and the push then refused the board for
            // having no name. The executor addresses the row with the
            // session's resolved uid when the probe anchored none.
            if caps.can_rename {
                commands.push(SetupCommand::WriteRegistry {
                    hardware_uid: provision
                        .probe
                        .as_ref()
                        .and_then(|probe| probe.hardware_uid.clone()),
                    hardware_origin: provision
                        .probe
                        .as_ref()
                        .and_then(|probe| probe.hardware_origin.clone()),
                    name: provision.name.clone(),
                    board_id: provision.board_id.clone(),
                });
            }
            commands.push(SetupCommand::PushProject {
                project_uid: project_uid.clone(),
            });
            SetupStep::with(
                SetupState::Provision(ProvisionState {
                    phase: ProvisionPhase::Pushing,
                    project_uid: Some(project_uid),
                    ..provision
                }),
                commands,
            )
        }
        (SetupState::Provision(provision), SetupEvent::PushCompleted) => {
            if provision.phase != ProvisionPhase::Pushing {
                return SetupStep::go(SetupState::Provision(provision));
            }
            SetupStep::with(
                SetupState::DeviceHome {
                    project_uid: provision.project_uid,
                    adopted: false,
                },
                vec![SetupCommand::OpenDeviceHome],
            )
        }

        // ✕ at PROVISION on hardware: the flash ALREADY LANDED, so this is
        // not a cancel — the board is alive, running our firmware, and
        // belongs on the roster. Releasing its port here is what left a
        // freshly flashed board reading "not connected" one click after
        // it was flashed (G2 walk, 2026-08-05). Nothing to mark, nothing
        // to release: the takeover ends and the card carries on with its
        // own body. (Listed BEFORE the cross-cutting close so it wins.)
        (SetupState::Provision(_), SetupEvent::CloseRequested) if caps.needs_connect => {
            SetupStep::go(closed(CloseReason::LeftConnected))
        }

        // ---- cross-cutting -------------------------------------------------
        // Port lost in any hardware state that holds one, except the flash
        // states (handled above as a flash failure) and FLASH_FAILED, whose
        // own retry guidance ASKS for a replug.
        (state, SetupEvent::PortLost)
            if caps.needs_connect
                && state.holds_port()
                && !matches!(state, SetupState::FlashFailed { .. }) =>
        {
            SetupStep::with(
                SetupState::ConnectIntro {
                    hint: ConnectHint::Disconnected,
                },
                vec![SetupCommand::ReleasePort],
            )
        }
        // ✕ anywhere else: nothing was written, so nothing is left behind.
        // ABANDON_GUARD is excluded because it IS the answer to ✕ — asking
        // again while the sheet is open must not slip past its own guard.
        (state, SetupEvent::CloseRequested)
            if !matches!(
                state,
                SetupState::DeviceHome { .. }
                    | SetupState::Closed { .. }
                    | SetupState::AbandonGuard { .. }
            ) =>
        {
            let release = caps.needs_connect && state.holds_port();
            SetupStep::with(closed(CloseReason::Cancelled), release_port_if(release))
        }

        // Everything else is inert (see this function's doc).
        (state, _) => SetupStep::go(state),
    }
}

const PORT_LOST_DURING_FLASH: &str = "the device disconnected while the firmware was being written";

fn connect_intro() -> SetupState {
    SetupState::ConnectIntro {
        hint: ConnectHint::None,
    }
}

fn closed(reason: CloseReason) -> SetupState {
    SetupState::Closed { reason }
}

/// The adopt write: the sighting, and only when the probe anchored an
/// identity. An anonymous board is remembered by nothing.
fn record_sighting_for(probe: &BoardProbe) -> Vec<SetupCommand> {
    match probe.hardware_uid.clone() {
        Some(hardware_uid) => vec![SetupCommand::RecordSighting { hardware_uid }],
        None => Vec::new(),
    }
}

fn release_port_if(release: bool) -> Vec<SetupCommand> {
    if release {
        vec![SetupCommand::ReleasePort]
    } else {
        Vec::new()
    }
}

/// PROBING's four-way branch — the verdict enum's whole purpose.
fn route_probe(probe: BoardProbe, preseeded_board: Option<String>) -> SetupState {
    match probe.verdict {
        BoardVerdict::LightPlayer { .. } => SetupState::AlreadyLp { probe },
        BoardVerdict::Wled { .. } => SetupState::WledFound { probe },
        BoardVerdict::Unresponsive { .. } => SetupState::ProbeFailed { probe },
        BoardVerdict::Blank { .. } => {
            let selected = preseeded_board.filter(|board_id| chip_allows(&probe, board_id));
            SetupState::BoardPick(BoardPickState {
                probe: Some(probe),
                selected,
                replaces_firmware: false,
            })
        }
    }
}

/// Whether a BOARD_FIRST choice survives contact with the probed chip. An
/// unidentified chip contradicts nothing, so the choice stands; a chip that
/// disagrees drops it rather than pre-selecting a board that cannot be
/// flashed.
fn chip_allows(probe: &BoardProbe, board_id: &str) -> bool {
    let Some(detected) = probe.detected_chip.as_deref() else {
        return true;
    };
    board_by_id(board_id).is_some_and(|board| lpa_link::chip_ids_match(detected, &board.family))
}

fn enter_provision(
    context: &SetupContext,
    board_id: String,
    probe: Option<BoardProbe>,
) -> SetupState {
    let name = if context.capabilities.can_rename {
        let remembered = probe
            .as_ref()
            .and_then(|probe| probe.verdict.known())
            .map(|row| row.name.clone());
        // A board must not collide with the name it already owns.
        let taken: Vec<String> = context
            .taken_names
            .iter()
            .filter(|name| Some(*name) != remembered.as_ref())
            .cloned()
            .collect();
        derive_device_name(
            remembered.as_deref(),
            board_label(&board_id),
            &context.stamp,
            &taken,
        )
    } else {
        String::new()
    };
    SetupState::Provision(ProvisionState {
        board_id,
        probe,
        name,
        phase: ProvisionPhase::Editing,
        project_uid: None,
    })
}

/// The catalog's human name for a board, or the id when the catalog does
/// not know it (design §7.6 — there is no short-label field to prefer).
fn board_label(board_id: &str) -> &str {
    board_by_id(board_id).map_or(board_id, |board| board.display_name.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::places::RegisteredDevice;
    use crate::app::setup_flow::event::{SetupEvent, SetupEventKind};
    use crate::app::setup_flow::state::SetupStateKind;

    const STAMP: &str = "2026-08-04-1421";
    const C6: &str = "seeed/xiao-esp32-c6";
    const S3: &str = "espressif/esp32-s3-devkitc-1";
    const MAC: &str = "aa:bb:cc:dd:ee:ff";

    fn hardware() -> SetupContext {
        SetupContext::new(SetupCapabilities::HARDWARE, STAMP)
    }

    fn simulator() -> SetupContext {
        SetupContext::new(SetupCapabilities::SIMULATOR, STAMP)
    }

    fn hardware_uid() -> String {
        crate::app::places::HardwareId::from_base_mac(MAC)
            .unwrap()
            .device_uid()
            .to_string()
    }

    fn probe(verdict: BoardVerdict) -> BoardProbe {
        BoardProbe {
            verdict,
            detected_chip: Some("esp32c6".to_string()),
            hardware_uid: Some(hardware_uid()),
            hardware_origin: Some("efuse:aa:bb:cc:dd:ee:ff".to_string()),
        }
    }

    fn blank() -> BoardProbe {
        probe(BoardVerdict::Blank { known: None })
    }

    fn remembered(name: &str) -> RegisteredDevice {
        RegisteredDevice {
            uid: hardware_uid(),
            name: name.to_string(),
            transport: "USB".to_string(),
            last_seen_at: 1.0,
            association: None,
            board_id: Some(C6.to_string()),
            hardware_id: Some("efuse:aa:bb:cc:dd:ee:ff".to_string()),
            previous_uids: Vec::new(),
        }
    }

    /// A representative state of each kind, built so the primary
    /// transition out of it can fire.
    fn state_of(kind: SetupStateKind) -> SetupState {
        match kind {
            SetupStateKind::ConnectIntro => connect_intro(),
            SetupStateKind::BoardFirst => SetupState::BoardFirst {
                chosen: Some(C6.to_string()),
            },
            SetupStateKind::PortPicking => SetupState::PortPicking {
                preseeded_board: None,
            },
            SetupStateKind::Probing => SetupState::Probing {
                preseeded_board: None,
            },
            SetupStateKind::BoardPick => SetupState::BoardPick(BoardPickState {
                probe: Some(blank()),
                selected: Some(C6.to_string()),
                replaces_firmware: false,
            }),
            SetupStateKind::WledFound => SetupState::WledFound {
                probe: probe(BoardVerdict::Wled { known: None }),
            },
            SetupStateKind::AlreadyLp => SetupState::AlreadyLp {
                probe: probe(BoardVerdict::LightPlayer { known: None }),
            },
            SetupStateKind::ProbeFailed => SetupState::ProbeFailed {
                probe: probe(BoardVerdict::Unresponsive { known: None }),
            },
            SetupStateKind::Flashing => SetupState::Flashing {
                board_id: C6.to_string(),
                probe: Some(blank()),
                attempt: 1,
            },
            SetupStateKind::FlashFailed => SetupState::FlashFailed {
                board_id: C6.to_string(),
                probe: Some(blank()),
                attempt: 1,
                detail: "boom".to_string(),
            },
            SetupStateKind::AbandonGuard => SetupState::AbandonGuard {
                board_id: C6.to_string(),
                probe: Some(blank()),
                attempt: 1,
            },
            SetupStateKind::Provision => SetupState::Provision(ProvisionState {
                board_id: C6.to_string(),
                probe: Some(blank()),
                name: "XIAO ESP32-C6 · Aug 4".to_string(),
                phase: ProvisionPhase::Editing,
                project_uid: None,
            }),
            SetupStateKind::DeviceHome => SetupState::DeviceHome {
                project_uid: Some("prj_1".to_string()),
                adopted: false,
            },
            SetupStateKind::Closed => closed(CloseReason::Cancelled),
        }
    }

    fn event_of(kind: SetupEventKind) -> SetupEvent {
        match kind {
            SetupEventKind::ItsConnected => SetupEvent::ItsConnected,
            SetupEventKind::PickBoardFirst => SetupEvent::PickBoardFirst,
            SetupEventKind::ItsPluggedIn => SetupEvent::ItsPluggedIn,
            SetupEventKind::Back => SetupEvent::Back,
            SetupEventKind::BoardChosen => SetupEvent::BoardChosen {
                board_id: S3.to_string(),
            },
            SetupEventKind::Confirm => SetupEvent::Confirm,
            SetupEventKind::PortGranted => SetupEvent::PortGranted,
            SetupEventKind::PortPickerCancelled => SetupEvent::PortPickerCancelled,
            SetupEventKind::PortPickerEmpty => SetupEvent::PortPickerEmpty,
            SetupEventKind::ProbeCompleted => SetupEvent::ProbeCompleted { probe: blank() },
            SetupEventKind::PortLost => SetupEvent::PortLost,
            SetupEventKind::WipeAndSetUp => SetupEvent::WipeAndSetUp,
            SetupEventKind::AdoptDone => SetupEvent::AdoptDone,
            SetupEventKind::AdoptAndOpen => SetupEvent::AdoptAndOpen,
            SetupEventKind::SetUpFresh => SetupEvent::SetUpFresh,
            SetupEventKind::Retry => SetupEvent::Retry,
            SetupEventKind::FlashSucceeded => SetupEvent::FlashSucceeded,
            SetupEventKind::FlashFailed => SetupEvent::FlashFailed {
                detail: "boom".to_string(),
            },
            SetupEventKind::NameEdited => SetupEvent::NameEdited {
                name: "Porch sign".to_string(),
            },
            SetupEventKind::ProjectGenerated => SetupEvent::ProjectGenerated {
                project_uid: "prj_1".to_string(),
            },
            SetupEventKind::PushCompleted => SetupEvent::PushCompleted,
            SetupEventKind::CloseRequested => SetupEvent::CloseRequested,
            SetupEventKind::KeepFlashing => SetupEvent::KeepFlashing,
            SetupEventKind::Abandon => SetupEvent::Abandon,
            // Deliberately carries a board: on the HARDWARE context this
            // table walks, even a fully-inferred board must leave the
            // flow where it is — a board with no firmware on it is not
            // set up by a project landing on the simulator.
            SetupEventKind::SetUpElsewhere => SetupEvent::SetUpElsewhere {
                board_id: Some(S3.to_string()),
            },
        }
    }

    /// The design doc's §2 table, transcribed. `None` = inert (state
    /// unchanged, no commands). Every `(state, event)` pair not listed for
    /// a state is asserted inert by
    /// [`every_state_event_pair_matches_the_design_doc`].
    fn expected_hardware(
        state: SetupStateKind,
        event: SetupEventKind,
    ) -> Option<(SetupStateKind, &'static [&'static str])> {
        use SetupEventKind as E;
        use SetupStateKind as S;
        let row: &[(E, S, &[&str])] = match state {
            S::ConnectIntro => &[
                (E::ItsConnected, S::PortPicking, &["request-port"]),
                (E::PickBoardFirst, S::BoardFirst, &[]),
                (E::CloseRequested, S::Closed, &[]),
            ],
            S::BoardFirst => &[
                (E::BoardChosen, S::BoardFirst, &[]),
                (E::ItsPluggedIn, S::PortPicking, &["request-port"]),
                (E::Back, S::ConnectIntro, &[]),
                (E::CloseRequested, S::Closed, &[]),
            ],
            S::PortPicking => &[
                (E::PortGranted, S::Probing, &["probe-board"]),
                (E::PortPickerCancelled, S::ConnectIntro, &[]),
                (E::PortPickerEmpty, S::ConnectIntro, &[]),
                (E::CloseRequested, S::Closed, &[]),
            ],
            S::Probing => &[
                (E::ProbeCompleted, S::BoardPick, &[]),
                (E::PortLost, S::ConnectIntro, &["release-port"]),
                (E::CloseRequested, S::Closed, &["release-port"]),
            ],
            S::BoardPick => &[
                (E::BoardChosen, S::BoardPick, &[]),
                (E::Confirm, S::Flashing, &["flash"]),
                (E::Back, S::ConnectIntro, &["release-port"]),
                (E::PortLost, S::ConnectIntro, &["release-port"]),
                (E::CloseRequested, S::Closed, &["release-port"]),
            ],
            S::WledFound => &[
                (E::WipeAndSetUp, S::BoardPick, &[]),
                (E::Back, S::ConnectIntro, &["release-port"]),
                (E::PortLost, S::ConnectIntro, &["release-port"]),
                (E::CloseRequested, S::Closed, &["release-port"]),
            ],
            S::AlreadyLp => &[
                // Adopt ends the flow where the user is: no lens attach,
                // and no port release either (the board stays on the
                // roster with its session).
                (E::AdoptDone, S::Closed, &["record-sighting"]),
                (
                    E::AdoptAndOpen,
                    S::DeviceHome,
                    &["record-sighting", "open-device-home"],
                ),
                (E::SetUpFresh, S::BoardPick, &[]),
                (E::PortLost, S::ConnectIntro, &["release-port"]),
                (E::CloseRequested, S::Closed, &["release-port"]),
            ],
            S::ProbeFailed => &[
                (E::Retry, S::Probing, &["probe-board"]),
                (E::PickBoardFirst, S::BoardFirst, &["release-port"]),
                (E::Back, S::ConnectIntro, &["release-port"]),
                (E::PortLost, S::ConnectIntro, &["release-port"]),
                (E::CloseRequested, S::Closed, &["release-port"]),
            ],
            S::Flashing => &[
                (E::FlashSucceeded, S::Provision, &[]),
                (E::FlashFailed, S::FlashFailed, &[]),
                (E::PortLost, S::FlashFailed, &[]),
                (E::CloseRequested, S::AbandonGuard, &[]),
            ],
            S::FlashFailed => &[
                (E::Retry, S::Flashing, &["flash"]),
                (
                    E::Abandon,
                    S::Closed,
                    &["mark-incomplete-flash", "release-port"],
                ),
                (
                    E::CloseRequested,
                    S::Closed,
                    &["mark-incomplete-flash", "release-port"],
                ),
            ],
            S::AbandonGuard => &[
                (E::KeepFlashing, S::Flashing, &[]),
                (
                    E::Abandon,
                    S::Closed,
                    &["mark-incomplete-flash", "release-port"],
                ),
                (E::FlashSucceeded, S::Provision, &[]),
                (E::FlashFailed, S::FlashFailed, &[]),
                (E::PortLost, S::FlashFailed, &[]),
            ],
            S::Provision => &[
                (E::NameEdited, S::Provision, &[]),
                (E::Confirm, S::Provision, &["generate-project"]),
                // from Editing, a late ProjectGenerated/PushCompleted is
                // inert — the phase-ordered variants are tested separately
                (E::PortLost, S::ConnectIntro, &["release-port"]),
                // ✕ after a landed flash keeps the board: no release.
                (E::CloseRequested, S::Closed, &[]),
            ],
            // Terminal: the card owns the surface from here.
            S::DeviceHome | S::Closed => &[],
        };
        row.iter()
            .find(|(candidate, _, _)| *candidate == event)
            .map(|(_, next, commands)| (*next, *commands))
    }

    #[test]
    fn every_state_event_pair_matches_the_design_doc() {
        let context = hardware();
        let mut checked = 0;
        for state_kind in SetupStateKind::ALL {
            for event_kind in SetupEventKind::ALL {
                let step = reduce(&context, state_of(state_kind), event_of(event_kind));
                let labels: Vec<&str> = step.commands.iter().map(SetupCommand::label).collect();
                match expected_hardware(state_kind, event_kind) {
                    Some((next, commands)) => {
                        assert_eq!(
                            step.state.kind(),
                            next,
                            "{} —{}→ expected {}",
                            state_kind.label(),
                            event_kind.label(),
                            next.label()
                        );
                        assert_eq!(
                            labels,
                            commands.to_vec(),
                            "{} —{}→ commands",
                            state_kind.label(),
                            event_kind.label()
                        );
                    }
                    None => {
                        assert_eq!(
                            step.state,
                            state_of(state_kind),
                            "{} —{}→ must be inert",
                            state_kind.label(),
                            event_kind.label()
                        );
                        assert!(
                            labels.is_empty(),
                            "{} —{}→ inert pairs ask for nothing, got {labels:?}",
                            state_kind.label(),
                            event_kind.label()
                        );
                    }
                }
                checked += 1;
            }
        }
        assert_eq!(
            checked,
            SetupStateKind::ALL.len() * SetupEventKind::ALL.len(),
            "the whole product is walked"
        );
    }

    #[test]
    fn probing_routes_every_verdict() {
        let context = hardware();
        let cases = [
            (
                BoardVerdict::Blank { known: None },
                SetupStateKind::BoardPick,
            ),
            (
                BoardVerdict::Wled { known: None },
                SetupStateKind::WledFound,
            ),
            (
                BoardVerdict::LightPlayer { known: None },
                SetupStateKind::AlreadyLp,
            ),
            (
                BoardVerdict::Unresponsive { known: None },
                SetupStateKind::ProbeFailed,
            ),
        ];
        for (verdict, expected) in cases {
            let step = reduce(
                &context,
                SetupState::Probing {
                    preseeded_board: None,
                },
                SetupEvent::ProbeCompleted {
                    probe: probe(verdict.clone()),
                },
            );
            assert_eq!(step.state.kind(), expected, "{}", verdict.label());
            assert!(step.commands.is_empty(), "routing asks for nothing");
        }
    }

    #[test]
    fn recognition_rides_every_verdict_into_its_state() {
        let context = hardware();
        for verdict in [
            BoardVerdict::Blank {
                known: Some(remembered("Porch sign")),
            },
            BoardVerdict::Wled {
                known: Some(remembered("Porch sign")),
            },
            BoardVerdict::LightPlayer {
                known: Some(remembered("Porch sign")),
            },
        ] {
            let label = verdict.label();
            let step = reduce(
                &context,
                SetupState::Probing {
                    preseeded_board: None,
                },
                SetupEvent::ProbeCompleted {
                    probe: probe(verdict),
                },
            );
            assert_eq!(
                step.state
                    .probe()
                    .and_then(|probe| probe.verdict.known())
                    .map(|row| row.name.as_str()),
                Some("Porch sign"),
                "{label} must carry the recognition into its state"
            );
        }
    }

    #[test]
    fn a_board_first_choice_preseeds_only_a_compatible_chip() {
        let context = hardware();
        // detected esp32c6 + a C6 board: the choice stands.
        let step = reduce(
            &context,
            SetupState::Probing {
                preseeded_board: Some(C6.to_string()),
            },
            SetupEvent::ProbeCompleted { probe: blank() },
        );
        assert_eq!(step.state.kind(), SetupStateKind::BoardPick);
        let SetupState::BoardPick(pick) = step.state else {
            unreachable!()
        };
        assert_eq!(pick.selected.as_deref(), Some(C6));

        // detected esp32c6 + an S3 board: the choice is dropped rather
        // than pre-selecting a board that cannot be flashed.
        let step = reduce(
            &context,
            SetupState::Probing {
                preseeded_board: Some(S3.to_string()),
            },
            SetupEvent::ProbeCompleted { probe: blank() },
        );
        let SetupState::BoardPick(pick) = step.state else {
            unreachable!()
        };
        assert_eq!(pick.selected, None);
    }

    #[test]
    fn an_unidentified_chip_contradicts_no_board_first_choice() {
        let context = hardware();
        let step = reduce(
            &context,
            SetupState::Probing {
                preseeded_board: Some(S3.to_string()),
            },
            SetupEvent::ProbeCompleted {
                probe: BoardProbe {
                    detected_chip: None,
                    ..blank()
                },
            },
        );
        let SetupState::BoardPick(pick) = step.state else {
            unreachable!()
        };
        assert_eq!(pick.selected.as_deref(), Some(S3));
    }

    #[test]
    fn confirm_without_a_board_is_not_armed() {
        let context = hardware();
        let state = SetupState::BoardPick(BoardPickState {
            probe: Some(blank()),
            selected: None,
            replaces_firmware: false,
        });
        let step = reduce(&context, state.clone(), SetupEvent::Confirm);
        assert_eq!(step.state, state);
        assert!(step.commands.is_empty());
    }

    #[test]
    fn wiping_wled_and_setting_up_fresh_both_flag_the_replacement() {
        let context = hardware();
        for (state, event) in [
            (
                SetupState::WledFound {
                    probe: probe(BoardVerdict::Wled { known: None }),
                },
                SetupEvent::WipeAndSetUp,
            ),
            (
                SetupState::AlreadyLp {
                    probe: probe(BoardVerdict::LightPlayer { known: None }),
                },
                SetupEvent::SetUpFresh,
            ),
        ] {
            let step = reduce(&context, state, event);
            let SetupState::BoardPick(pick) = step.state else {
                panic!("both land on the board pick")
            };
            assert!(pick.replaces_firmware, "the confirmation has to warn");
            assert!(pick.probe.is_some(), "the probe evidence survives");
        }
    }

    #[test]
    fn the_provision_name_is_derived_and_editable() {
        let context = hardware().with_taken_names(vec!["XIAO ESP32-C6 · Aug 4".to_string()]);
        let step = reduce(
            &context,
            SetupState::Flashing {
                board_id: C6.to_string(),
                probe: Some(blank()),
                attempt: 1,
            },
            SetupEvent::FlashSucceeded,
        );
        let SetupState::Provision(provision) = &step.state else {
            unreachable!()
        };
        assert_eq!(provision.name, "XIAO ESP32-C6 · Aug 4 2");

        let step = reduce(
            &context,
            step.state,
            SetupEvent::NameEdited {
                name: "Porch sign".to_string(),
            },
        );
        let SetupState::Provision(provision) = step.state else {
            unreachable!()
        };
        assert_eq!(provision.name, "Porch sign");
    }

    #[test]
    fn a_remembered_board_keeps_its_name_without_colliding_with_itself() {
        let context = hardware().with_taken_names(vec!["Porch sign".to_string()]);
        let step = reduce(
            &context,
            SetupState::Flashing {
                board_id: C6.to_string(),
                probe: Some(probe(BoardVerdict::Blank {
                    known: Some(remembered("Porch sign")),
                })),
                attempt: 1,
            },
            SetupEvent::FlashSucceeded,
        );
        let SetupState::Provision(provision) = step.state else {
            unreachable!()
        };
        assert_eq!(provision.name, "Porch sign");
    }

    #[test]
    fn provision_work_is_phase_ordered_and_not_repeatable() {
        let context = hardware();
        let mut flow = SetupFlow {
            context: context.clone(),
            state: state_of(SetupStateKind::Provision),
        };
        assert_eq!(
            flow.handle(SetupEvent::Confirm)
                .iter()
                .map(SetupCommand::label)
                .collect::<Vec<_>>(),
            vec!["generate-project"]
        );
        // A second click while generating must not generate twice.
        assert!(flow.handle(SetupEvent::Confirm).is_empty());
        // A push report before the generator answers is inert.
        assert!(flow.handle(SetupEvent::PushCompleted).is_empty());

        let commands = flow.handle(SetupEvent::ProjectGenerated {
            project_uid: "prj_1".to_string(),
        });
        assert_eq!(
            commands.iter().map(SetupCommand::label).collect::<Vec<_>>(),
            vec!["write-registry", "push-project"]
        );
        // Generated twice: the second is inert.
        assert!(
            flow.handle(SetupEvent::ProjectGenerated {
                project_uid: "prj_2".to_string(),
            })
            .is_empty()
        );

        assert_eq!(
            flow.handle(SetupEvent::PushCompleted)
                .iter()
                .map(SetupCommand::label)
                .collect::<Vec<_>>(),
            vec!["open-device-home"]
        );
        assert_eq!(
            flow.state(),
            &SetupState::DeviceHome {
                project_uid: Some("prj_1".to_string()),
                adopted: false,
            }
        );
    }

    #[test]
    fn a_board_the_probe_could_not_anchor_still_asks_for_the_registry_write() {
        // G2 blank-C6 walk, 2026-08-05: this used to emit NO write, on the
        // reasoning that an un-anchored board has no key to be remembered
        // under. True at probe time — and wrong by provision time, because
        // the flash in between is what gave the board its identity. The
        // reducer asks with an ADVISORY uid (`None` here); the executor,
        // which can see the live session, addresses the row (or refuses
        // it — `executor::tests::a_board_no_identity_anchors_writes_no_row_at_all`).
        let context = hardware();
        let state = SetupState::Provision(ProvisionState {
            board_id: C6.to_string(),
            probe: Some(BoardProbe {
                hardware_uid: None,
                hardware_origin: None,
                ..blank()
            }),
            name: "Nameless".to_string(),
            phase: ProvisionPhase::Generating,
            project_uid: None,
        });
        let step = reduce(
            &context,
            state,
            SetupEvent::ProjectGenerated {
                project_uid: "prj_1".to_string(),
            },
        );
        assert_eq!(
            step.commands
                .iter()
                .map(SetupCommand::label)
                .collect::<Vec<_>>(),
            vec!["write-registry", "push-project"]
        );
        let Some(SetupCommand::WriteRegistry { hardware_uid, .. }) = step.commands.first() else {
            panic!("the write leads")
        };
        assert_eq!(
            *hardware_uid, None,
            "the reducer does not invent an identity it cannot see"
        );
    }

    // ---- capability boundary (R2) -----------------------------------------

    #[test]
    fn a_target_that_needs_no_connect_opens_on_the_board_pick() {
        let flow = SetupFlow::start(simulator());
        assert_eq!(flow.state().kind(), SetupStateKind::BoardPick);
        assert_eq!(flow.state().probe(), None, "no probe evidence to filter by");

        let flow = SetupFlow::start(hardware());
        assert_eq!(flow.state().kind(), SetupStateKind::ConnectIntro);
    }

    #[test]
    fn a_target_that_needs_no_flash_confirms_straight_into_provision() {
        let context = simulator();
        let state = SetupState::BoardPick(BoardPickState {
            probe: None,
            selected: Some(C6.to_string()),
            replaces_firmware: false,
        });
        let step = reduce(&context, state, SetupEvent::Confirm);
        assert_eq!(step.state.kind(), SetupStateKind::Provision);
        assert!(step.commands.is_empty(), "nothing to flash");
    }

    #[test]
    fn a_target_that_cannot_be_renamed_carries_no_name_and_writes_no_registry() {
        let context = simulator();
        let step = reduce(
            &context,
            SetupState::BoardPick(BoardPickState {
                probe: None,
                selected: Some(C6.to_string()),
                replaces_firmware: false,
            }),
            SetupEvent::Confirm,
        );
        let SetupState::Provision(provision) = &step.state else {
            unreachable!()
        };
        assert_eq!(provision.name, "", "the sim shows no name field");

        // and the name field cannot be edited into existence
        let step = reduce(
            &context,
            step.state.clone(),
            SetupEvent::NameEdited {
                name: "Nope".to_string(),
            },
        );
        let SetupState::Provision(provision) = step.state else {
            unreachable!()
        };
        assert_eq!(provision.name, "");
    }

    #[test]
    fn a_target_with_no_port_never_asks_to_release_one() {
        let context = simulator();
        for event in [SetupEvent::CloseRequested, SetupEvent::Back] {
            let step = reduce(
                &context,
                SetupState::BoardPick(BoardPickState::default()),
                event.clone(),
            );
            assert_eq!(step.state.kind(), SetupStateKind::Closed, "{event:?}");
            assert!(step.commands.is_empty(), "{event:?} holds no port");
        }
    }

    // ---- set up by another route (G1b ruling 6) ----------------------------

    #[test]
    fn a_board_the_landing_could_infer_ends_the_picker() {
        let step = reduce(
            &simulator(),
            SetupState::BoardPick(BoardPickState::default()),
            SetupEvent::SetUpElsewhere {
                board_id: Some(C6.to_string()),
            },
        );
        assert_eq!(step.state, closed(CloseReason::SetUpElsewhere));
        assert!(
            step.commands.is_empty(),
            "the landing already did the work; there is nothing to ask for"
        );
    }

    #[test]
    fn a_landing_that_infers_no_board_leaves_the_picker_asking() {
        let picker = SetupState::BoardPick(BoardPickState::default());
        let step = reduce(
            &simulator(),
            picker.clone(),
            SetupEvent::SetUpElsewhere { board_id: None },
        );
        assert_eq!(
            step.state, picker,
            "an untargeted project answers nothing, so the picker still asks"
        );
        assert!(step.commands.is_empty());
    }

    #[test]
    fn a_target_that_still_needs_firmware_is_not_set_up_by_someone_elses_landing() {
        // The capability, not the kind (R2): a blank board mid-flow keeps
        // its picker no matter what lands on the simulator next door.
        let picker = SetupState::BoardPick(BoardPickState {
            probe: Some(blank()),
            selected: None,
            replaces_firmware: false,
        });
        let step = reduce(
            &hardware(),
            picker.clone(),
            SetupEvent::SetUpElsewhere {
                board_id: Some(C6.to_string()),
            },
        );
        assert_eq!(step.state, picker);
        assert!(step.commands.is_empty());
    }

    #[test]
    fn a_landing_after_the_board_pick_is_inert() {
        // PROVISION is already past the question, and the states beyond it
        // are terminal. Only the picker has anything to stand down.
        for kind in SetupStateKind::ALL {
            if kind == SetupStateKind::BoardPick {
                continue;
            }
            let state = state_of(kind);
            let step = reduce(
                &simulator(),
                state.clone(),
                SetupEvent::SetUpElsewhere {
                    board_id: Some(C6.to_string()),
                },
            );
            assert_eq!(step.state, state, "{} must be inert", kind.label());
            assert!(step.commands.is_empty(), "{}", kind.label());
        }
    }

    // ---- golden command traces (F7) ---------------------------------------

    fn trace(context: SetupContext, events: Vec<SetupEvent>) -> (SetupFlow, Vec<String>) {
        let mut flow = SetupFlow::start(context);
        let mut labels = Vec::new();
        for event in events {
            for command in flow.handle(event) {
                labels.push(command.label().to_string());
            }
        }
        (flow, labels)
    }

    #[test]
    fn golden_hardware_blank_board_happy_path() {
        let (flow, commands) = trace(
            hardware(),
            vec![
                SetupEvent::ItsConnected,
                SetupEvent::PortGranted,
                SetupEvent::ProbeCompleted { probe: blank() },
                SetupEvent::BoardChosen {
                    board_id: C6.to_string(),
                },
                SetupEvent::Confirm,
                SetupEvent::FlashSucceeded,
                SetupEvent::Confirm,
                SetupEvent::ProjectGenerated {
                    project_uid: "prj_1".to_string(),
                },
                SetupEvent::PushCompleted,
            ],
        );
        assert_eq!(
            commands,
            vec![
                "request-port",
                "probe-board",
                "flash",
                "generate-project",
                "write-registry",
                "push-project",
                "open-device-home",
            ]
        );
        assert_eq!(
            flow.state(),
            &SetupState::DeviceHome {
                project_uid: Some("prj_1".to_string()),
                adopted: false,
            }
        );
    }

    #[test]
    fn golden_simulator_path() {
        let (flow, commands) = trace(
            simulator(),
            vec![
                SetupEvent::BoardChosen {
                    board_id: C6.to_string(),
                },
                SetupEvent::Confirm,
                SetupEvent::Confirm,
                SetupEvent::ProjectGenerated {
                    project_uid: "prj_1".to_string(),
                },
                SetupEvent::PushCompleted,
            ],
        );
        // No connect, no flash, no registry write — the same states and the
        // same code, minus what the target cannot do.
        assert_eq!(
            commands,
            vec!["generate-project", "push-project", "open-device-home"]
        );
        assert_eq!(flow.state().kind(), SetupStateKind::DeviceHome);
    }

    #[test]
    fn golden_adopt_path_writes_nothing_but_the_sighting() {
        // G2 follow-up 2026-08-05: "Done" adopts and ENDS — no lens
        // attach, and no port release either. The user stays where they
        // are, and the board is on the roster with its session, which is
        // the whole content of "it joins your roster as it is".
        let (flow, commands) = trace(
            hardware(),
            vec![
                SetupEvent::ItsConnected,
                SetupEvent::PortGranted,
                SetupEvent::ProbeCompleted {
                    probe: probe(BoardVerdict::LightPlayer {
                        known: Some(remembered("Porch sign")),
                    }),
                },
                SetupEvent::AdoptDone,
            ],
        );
        assert_eq!(
            commands,
            vec!["request-port", "probe-board", "record-sighting"]
        );
        assert_eq!(
            flow.state(),
            &SetupState::Closed {
                reason: CloseReason::Adopted
            }
        );
    }

    #[test]
    fn adopt_and_open_is_the_only_adopt_edge_that_navigates() {
        // The secondary CTA does what "Done" used to: the same sighting,
        // then the editor lensed to the board.
        let (flow, commands) = trace(
            hardware(),
            vec![
                SetupEvent::ItsConnected,
                SetupEvent::PortGranted,
                SetupEvent::ProbeCompleted {
                    probe: probe(BoardVerdict::LightPlayer {
                        known: Some(remembered("Porch sign")),
                    }),
                },
                SetupEvent::AdoptAndOpen,
            ],
        );
        assert_eq!(
            commands,
            vec![
                "request-port",
                "probe-board",
                "record-sighting",
                "open-device-home"
            ]
        );
        assert_eq!(
            flow.state(),
            &SetupState::DeviceHome {
                project_uid: None,
                adopted: true,
            }
        );
    }

    #[test]
    fn an_anonymous_board_is_adopted_without_a_registry_write() {
        // No hardware uid, no row: a board remembered by nothing is still
        // adoptable, it is just not remembered.
        let (flow, commands) = trace(
            hardware(),
            vec![
                SetupEvent::ItsConnected,
                SetupEvent::PortGranted,
                SetupEvent::ProbeCompleted {
                    probe: BoardProbe {
                        verdict: BoardVerdict::LightPlayer { known: None },
                        detected_chip: Some("esp32c6".to_string()),
                        hardware_uid: None,
                        hardware_origin: None,
                    },
                },
                SetupEvent::AdoptDone,
            ],
        );
        assert_eq!(commands, vec!["request-port", "probe-board"]);
        assert_eq!(
            flow.state(),
            &SetupState::Closed {
                reason: CloseReason::Adopted
            }
        );
    }

    #[test]
    fn golden_wled_wipe_path() {
        let (flow, commands) = trace(
            hardware(),
            vec![
                SetupEvent::ItsConnected,
                SetupEvent::PortGranted,
                SetupEvent::ProbeCompleted {
                    probe: probe(BoardVerdict::Wled {
                        known: Some(remembered("Porch sign")),
                    }),
                },
                SetupEvent::WipeAndSetUp,
                SetupEvent::BoardChosen {
                    board_id: C6.to_string(),
                },
                SetupEvent::Confirm,
                SetupEvent::FlashSucceeded,
            ],
        );
        assert_eq!(
            commands,
            vec!["request-port", "probe-board", "flash"],
            "the wipe IS the flash — no migration step exists"
        );
        let SetupState::Provision(provision) = flow.state() else {
            panic!("a wiped WLED board provisions like any other")
        };
        assert_eq!(
            provision.name, "Porch sign",
            "the board it used to be is remembered"
        );
    }

    #[test]
    fn golden_flash_fail_retry_twice_then_replug_guidance() {
        let (flow, commands) = trace(
            hardware(),
            vec![
                SetupEvent::ItsConnected,
                SetupEvent::PortGranted,
                SetupEvent::ProbeCompleted { probe: blank() },
                SetupEvent::BoardChosen {
                    board_id: C6.to_string(),
                },
                SetupEvent::Confirm,
                SetupEvent::FlashFailed {
                    detail: "espflash: timed out".to_string(),
                },
                SetupEvent::Retry,
                SetupEvent::FlashFailed {
                    detail: "espflash: timed out".to_string(),
                },
                SetupEvent::Retry,
            ],
        );
        assert_eq!(
            commands,
            vec!["request-port", "probe-board", "flash", "flash", "flash"]
        );
        assert_eq!(
            flow.state(),
            &SetupState::Flashing {
                board_id: C6.to_string(),
                probe: Some(blank()),
                attempt: 3,
            }
        );

        // The guidance itself: attempt 1 carries none, later attempts do.
        let mut flow = SetupFlow::start(hardware());
        for event in [
            SetupEvent::ItsConnected,
            SetupEvent::PortGranted,
            SetupEvent::ProbeCompleted { probe: blank() },
            SetupEvent::BoardChosen {
                board_id: C6.to_string(),
            },
        ] {
            flow.handle(event);
        }
        assert_eq!(
            flow.handle(SetupEvent::Confirm),
            vec![SetupCommand::Flash {
                board_id: C6.to_string(),
                attempt: 1,
                replug_guidance: false,
            }]
        );
        flow.handle(SetupEvent::FlashFailed {
            detail: "boom".to_string(),
        });
        assert_eq!(
            flow.handle(SetupEvent::Retry),
            vec![SetupCommand::Flash {
                board_id: C6.to_string(),
                attempt: 2,
                replug_guidance: true,
            }]
        );
        flow.handle(SetupEvent::FlashFailed {
            detail: "boom".to_string(),
        });
        assert_eq!(
            flow.handle(SetupEvent::Retry),
            vec![SetupCommand::Flash {
                board_id: C6.to_string(),
                attempt: 3,
                replug_guidance: true,
            }]
        );
    }

    #[test]
    fn abandoning_a_flash_leaves_the_board_marked_incomplete() {
        let mut flow = SetupFlow::start(hardware());
        for event in [
            SetupEvent::ItsConnected,
            SetupEvent::PortGranted,
            SetupEvent::ProbeCompleted { probe: blank() },
            SetupEvent::BoardChosen {
                board_id: C6.to_string(),
            },
            SetupEvent::Confirm,
        ] {
            flow.handle(event);
        }
        // ✕ during a flash opens the guard; the flash never paused.
        assert!(flow.handle(SetupEvent::CloseRequested).is_empty());
        assert_eq!(flow.state().kind(), SetupStateKind::AbandonGuard);
        assert!(flow.handle(SetupEvent::KeepFlashing).is_empty());
        assert_eq!(flow.state().kind(), SetupStateKind::Flashing);

        flow.handle(SetupEvent::CloseRequested);
        let commands = flow.handle(SetupEvent::Abandon);
        assert_eq!(
            commands.iter().map(SetupCommand::label).collect::<Vec<_>>(),
            vec!["mark-incomplete-flash", "release-port"]
        );
        assert_eq!(
            flow.state(),
            &SetupState::Closed {
                reason: CloseReason::IncompleteFlash
            }
        );
    }

    #[test]
    fn a_flash_that_lands_while_the_guard_is_open_still_provisions() {
        // The guard is a sheet over an operation that never stopped; the
        // sheet must not be able to strand a finished flash.
        let context = hardware();
        let step = reduce(
            &context,
            SetupState::AbandonGuard {
                board_id: C6.to_string(),
                probe: Some(blank()),
                attempt: 1,
            },
            SetupEvent::FlashSucceeded,
        );
        assert_eq!(step.state.kind(), SetupStateKind::Provision);
    }

    #[test]
    fn a_lost_port_during_a_flash_is_a_flash_failure() {
        let context = hardware();
        let step = reduce(
            &context,
            SetupState::Flashing {
                board_id: C6.to_string(),
                probe: Some(blank()),
                attempt: 1,
            },
            SetupEvent::PortLost,
        );
        let SetupState::FlashFailed { detail, .. } = step.state else {
            panic!("the link layer surfaces it that way")
        };
        assert_eq!(detail, PORT_LOST_DURING_FLASH);
    }

    #[test]
    fn a_lost_port_after_a_failed_flash_is_the_replug_we_asked_for() {
        let context = hardware();
        let state = SetupState::FlashFailed {
            board_id: C6.to_string(),
            probe: Some(blank()),
            attempt: 2,
            detail: "boom".to_string(),
        };
        let step = reduce(&context, state.clone(), SetupEvent::PortLost);
        assert_eq!(step.state, state, "the retry affordance survives a replug");
    }

    #[test]
    fn closing_before_a_flash_leaves_nothing_behind() {
        for kind in [
            SetupStateKind::ConnectIntro,
            SetupStateKind::BoardFirst,
            SetupStateKind::PortPicking,
            SetupStateKind::Probing,
            SetupStateKind::BoardPick,
            SetupStateKind::WledFound,
            SetupStateKind::AlreadyLp,
            SetupStateKind::ProbeFailed,
        ] {
            let step = reduce(&hardware(), state_of(kind), SetupEvent::CloseRequested);
            assert_eq!(
                step.state,
                closed(CloseReason::Cancelled),
                "{}",
                kind.label()
            );
            assert!(
                !step
                    .commands
                    .iter()
                    .any(|c| matches!(c, SetupCommand::MarkIncompleteFlash)),
                "{} wrote no firmware",
                kind.label()
            );
        }
    }

    #[test]
    fn closing_at_provision_keeps_the_flashed_board_connected() {
        // G2 walk, 2026-08-05: ✕ at PROVISION released the port, and a
        // board that had just been flashed successfully went straight to
        // "not connected" — with a Reconnect that then had to fight for
        // the port it had just been handed. The flash landed; the board is
        // alive; it stays.
        let step = reduce(
            &hardware(),
            state_of(SetupStateKind::Provision),
            SetupEvent::CloseRequested,
        );
        assert_eq!(step.state, closed(CloseReason::LeftConnected));
        assert!(
            step.commands.is_empty(),
            "nothing to release and nothing to mark, got {:?}",
            step.commands
                .iter()
                .map(SetupCommand::label)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_simulators_provision_close_is_still_a_plain_cancel() {
        // "Left connected" is a statement about a board on a wire. The sim
        // has neither, so its ✕ stays the ordinary cancel.
        let step = reduce(
            &simulator(),
            state_of(SetupStateKind::Provision),
            SetupEvent::CloseRequested,
        );
        assert_eq!(step.state, closed(CloseReason::Cancelled));
        assert!(step.commands.is_empty());
    }
}
