//! [`AppAgentHostBridge`]: Studio's implementation of the app agent's
//! [`lpa_agent::AppAgentHost`].
//!
//! Like the shader bridge, it serves a snapshot the controller refreshes
//! after every processed batch: the run future reads the cell, it never
//! reaches into the controller. Writes (P03 onward) ride the command queue
//! as ordinary ops, so they land where a user's edit lands.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::time::Duration;

use lpa_agent::{
    ActInput, ActOutcome, AppAgentHost, EditProjectInput, HostError, HostFuture,
    ProjectEditsOutcome, ReadInput,
};

use crate::app::agent::agent_controller::{AgentController, AgentTimerFactory};
use crate::app::agent::agent_op::AgentOp;
use crate::app::studio::studio_view_channel::CommandSender;
use crate::{ControllerId, OfferPath, StudioCommand, UiAction};

/// How long an `edit_project` batch may take to come back (node creates
/// and a save round-trip the runtime; a long batch on a device is slow).
const EDITS_ACK_BUDGET_MS: u32 = 60_000;

/// The poll step of the ack wait (the platform timer).
const EDITS_POLL_STEP_MS: u32 = 50;

/// How long an edit result waits for the engine to report on the edits
/// (the shader agent's verdict-chase budget, `ENGINE_VERDICT_BUDGET_MS`).
const SETTLE_BUDGET_MS: u32 = 2_000;

/// Fresh project reads a settled summary needs after the ack: the first
/// read can carry statuses from before the edits compiled (see the shader
/// bridge's `VerdictFence`).
const SETTLE_READS: i64 = 2;

/// The snapshot the app bridge serves.
#[derive(Clone, Debug, Default)]
pub struct AppAgentBridgeState {
    /// The focused readout of the app, refreshed after every batch.
    pub readout: crate::app::agent::app_agent_readout::AppReadoutSnapshot,
    /// Every offer path a readout the agent was shown has listed this
    /// session: an `act` on one of these that the tree no longer offers is
    /// "not offered any more", rather than unknown.
    pub shown: BTreeSet<OfferPath>,
    /// Facts the embedder knows that the view model does not show (yet):
    /// appended to every readout. Evals put the scenario's board line here.
    pub context_notes: Vec<String>,
    /// The last `edit_project` batch's answer, keyed by the bridge's seq.
    pub edits_ack: Option<(u64, Result<ProjectEditsOutcome, String>)>,
    /// The last `read`'s answer, keyed by the bridge's seq.
    pub read_ack: Option<(u64, Result<serde_json::Value, String>)>,
    /// The last `act`'s answer, keyed by the bridge's seq.
    pub act_ack: Option<(u64, Result<ActOutcome, String>)>,
    /// The open project's compact summary and the sync revision it was read
    /// at, refreshed after every batch while a run is in flight.
    pub project: Option<(i64, serde_json::Value)>,
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

impl AppAgentHostBridge {
    /// The project summary once the engine has read the edits back twice,
    /// or the latest one when the budget runs out (marked unsettled — never
    /// a blocked run).
    async fn settled_project(&mut self) -> serde_json::Value {
        let start = self
            .state
            .borrow()
            .project
            .as_ref()
            .map(|(revision, _)| *revision);
        let mut waited_ms = 0u32;
        loop {
            let latest = self.state.borrow().project.clone();
            if let Some((revision, summary)) = &latest
                && start.is_none_or(|start| *revision >= start + SETTLE_READS)
            {
                return summary.clone();
            }
            if waited_ms >= SETTLE_BUDGET_MS {
                let mut summary = latest
                    .map(|(_, summary)| summary)
                    .unwrap_or_else(|| serde_json::json!({}));
                summary["settled"] =
                    "unknown — the engine had not reported on these edits yet".into();
                return summary;
            }
            (self.timer.borrow_mut())(Duration::from_millis(u64::from(EDITS_POLL_STEP_MS))).await;
            waited_ms += EDITS_POLL_STEP_MS;
        }
    }
}

impl AppAgentHost for AppAgentHostBridge {
    /// One `read`: an `AgentOp::AppRead` on the command queue, answered in
    /// the shared cell (the controller reads what it holds — and fetches a
    /// def body it has not cached, which is why this is not synchronous).
    fn read<'a>(
        &'a mut self,
        input: &'a ReadInput,
    ) -> HostFuture<'a, Result<serde_json::Value, HostError>> {
        Box::pin(async move {
            self.seq += 1;
            let seq = self.seq;
            self.state.borrow_mut().read_ack = None;
            self.tx.send(StudioCommand::Action(UiAction::from_op(
                ControllerId::new(AgentController::NODE_ID),
                AgentOp::AppRead {
                    seq,
                    input: input.clone(),
                },
            )));
            let mut waited_ms = 0u32;
            loop {
                let ack = self.state.borrow().read_ack.clone();
                if let Some((ack_seq, result)) = ack
                    && ack_seq == seq
                {
                    return result.map_err(HostError::new);
                }
                if waited_ms >= EDITS_ACK_BUDGET_MS {
                    return Err(HostError::new("the read was not answered in time"));
                }
                (self.timer.borrow_mut())(Duration::from_millis(u64::from(EDITS_POLL_STEP_MS)))
                    .await;
                waited_ms += EDITS_POLL_STEP_MS;
            }
        })
    }

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
                    let mut outcome = result.map_err(HostError::new)?;
                    outcome.project = Some(self.settled_project().await);
                    return Ok(outcome);
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

    /// One `act`: an `AgentOp::AppAct` on the command queue — the
    /// controller looks the offer path up in the current offer tree and
    /// presses it or puts it on a card — answered in the shared cell.
    fn act<'a>(&'a mut self, input: &'a ActInput) -> HostFuture<'a, Result<ActOutcome, HostError>> {
        Box::pin(async move {
            self.seq += 1;
            let seq = self.seq;
            self.state.borrow_mut().act_ack = None;
            self.tx.send(StudioCommand::Action(UiAction::from_op(
                ControllerId::new(AgentController::NODE_ID),
                AgentOp::AppAct {
                    seq,
                    input: input.clone(),
                },
            )));
            let mut waited_ms = 0u32;
            loop {
                let ack = self.state.borrow().act_ack.clone();
                if let Some((ack_seq, result)) = ack
                    && ack_seq == seq
                {
                    return result.map_err(HostError::new);
                }
                if waited_ms >= EDITS_ACK_BUDGET_MS {
                    return Err(HostError::new("the action was not answered in time"));
                }
                (self.timer.borrow_mut())(Duration::from_millis(u64::from(EDITS_POLL_STEP_MS)))
                    .await;
                waited_ms += EDITS_POLL_STEP_MS;
            }
        })
    }

    fn readout(&mut self) -> String {
        let mut guard = self.state.borrow_mut();
        let state = &mut *guard;
        let mut out = state.readout.render();
        state
            .shown
            .extend(state.readout.offers.iter().map(|offer| offer.path.clone()));
        for note in &state.context_notes {
            out.push_str(note);
            out.push('\n');
        }
        out.trim_end().to_string()
    }
}
