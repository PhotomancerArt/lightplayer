//! [`AppAgentHostBridge`]: Studio's implementation of the app agent's
//! [`lpa_agent::AppAgentHost`].
//!
//! Like the shader bridge, it serves a snapshot the controller refreshes
//! after every processed batch: the run future reads the cell, it never
//! reaches into the controller. Writes (P03 onward) ride the command queue
//! as ordinary ops, so they land where a user's edit lands.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use lpa_agent::{AppAgentHost, EditProjectInput, HostError, HostFuture, ProjectEditsOutcome};

use crate::app::agent::agent_controller::{AgentController, AgentTimerFactory};
use crate::app::agent::agent_op::AgentOp;
use crate::app::studio::studio_view_channel::CommandSender;
use crate::{ControllerId, StudioCommand, UiAction};

/// How long an `edit_project` batch may take to come back (node creates
/// and a save round-trip the runtime; a long batch on a device is slow).
const EDITS_ACK_BUDGET_MS: u32 = 60_000;

/// The poll step of the ack wait (the platform timer).
const EDITS_POLL_STEP_MS: u32 = 50;

/// The snapshot the app bridge serves.
#[derive(Clone, Debug, Default)]
pub struct AppAgentBridgeState {
    /// The focused readout of the app, refreshed after every batch.
    pub readout: String,
    /// Facts the embedder knows that the view model does not show (yet):
    /// appended to every readout. Evals put the scenario's board line here.
    pub context_notes: Vec<String>,
    /// The last `edit_project` batch's answer, keyed by the bridge's seq.
    pub edits_ack: Option<(u64, Result<ProjectEditsOutcome, String>)>,
}

/// The host handed to the app agent's `AppToolset`.
pub struct AppAgentHostBridge {
    state: Rc<RefCell<AppAgentBridgeState>>,
    tx: CommandSender,
    /// The platform timer the ack wait polls on.
    timer: AgentTimerFactory,
    /// Correlation counter for `edit_project` dispatches.
    seq: u64,
}

impl AppAgentHostBridge {
    pub fn new(
        state: Rc<RefCell<AppAgentBridgeState>>,
        tx: CommandSender,
        timer: AgentTimerFactory,
    ) -> Self {
        Self {
            state,
            tx,
            timer,
            seq: 0,
        }
    }
}

impl AppAgentHost for AppAgentHostBridge {
    /// Dispatch the batch as ONE `AgentOp::ApplyProjectEdits` on the command
    /// queue — the same queue the user's clicks ride — and wait for its
    /// ack in the shared cell.
    fn apply_project_edits<'a>(
        &'a mut self,
        input: &'a EditProjectInput,
    ) -> HostFuture<'a, Result<ProjectEditsOutcome, HostError>> {
        Box::pin(async move {
            self.seq += 1;
            let seq = self.seq;
            self.state.borrow_mut().edits_ack = None;
            self.tx.send(StudioCommand::Action(
                UiAction::from_op(
                    ControllerId::new(AgentController::NODE_ID),
                    AgentOp::ApplyProjectEdits {
                        seq,
                        input: input.clone(),
                    },
                )
                .with_summary("Apply the assistant's project edits."),
            ));
            let mut waited_ms = 0u32;
            loop {
                let ack = self.state.borrow().edits_ack.clone();
                if let Some((ack_seq, result)) = ack
                    && ack_seq == seq
                {
                    return result.map_err(HostError::new);
                }
                if waited_ms >= EDITS_ACK_BUDGET_MS {
                    return Err(HostError::new(
                        "the project edits were not acknowledged in time",
                    ));
                }
                (self.timer.borrow_mut())(Duration::from_millis(u64::from(EDITS_POLL_STEP_MS)))
                    .await;
                waited_ms += EDITS_POLL_STEP_MS;
            }
        })
    }

    fn readout(&mut self) -> String {
        let state = self.state.borrow();
        let mut out = state.readout.clone();
        for note in &state.context_notes {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(note);
        }
        out
    }
}
