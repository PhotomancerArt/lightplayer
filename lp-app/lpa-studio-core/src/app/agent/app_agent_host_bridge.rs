//! [`AppAgentHostBridge`]: Studio's implementation of the app agent's
//! [`lpa_agent::AppAgentHost`].
//!
//! Like the shader bridge, it serves a snapshot the controller refreshes
//! after every processed batch: the run future reads the cell, it never
//! reaches into the controller. Writes (P03 onward) ride the command queue
//! as ordinary ops, so they land where a user's edit lands.

use std::cell::RefCell;
use std::rc::Rc;

use lpa_agent::AppAgentHost;

use crate::app::agent::agent_controller::AgentTimerFactory;
use crate::app::studio::studio_view_channel::CommandSender;

/// The snapshot the app bridge serves.
#[derive(Clone, Debug, Default)]
pub struct AppAgentBridgeState {
    /// The focused readout of the app, refreshed after every batch.
    pub readout: String,
    /// Facts the embedder knows that the view model does not show (yet):
    /// appended to every readout. Evals put the scenario's board line here.
    pub context_notes: Vec<String>,
}

/// The host handed to the app agent's `AppToolset`.
pub struct AppAgentHostBridge {
    state: Rc<RefCell<AppAgentBridgeState>>,
    // The write path (P03) dispatches through these.
    #[allow(
        dead_code,
        reason = "the edit tools (plan P03) dispatch through the command queue"
    )]
    tx: CommandSender,
    #[allow(
        dead_code,
        reason = "the post-edit wait (plan P04) polls on the platform timer"
    )]
    timer: AgentTimerFactory,
}

impl AppAgentHostBridge {
    pub fn new(
        state: Rc<RefCell<AppAgentBridgeState>>,
        tx: CommandSender,
        timer: AgentTimerFactory,
    ) -> Self {
        Self { state, tx, timer }
    }
}

impl AppAgentHost for AppAgentHostBridge {
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
