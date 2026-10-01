//! [`AppAgentSession`]: the one app-level agent conversation a page holds
//! (in memory for the page's life — plan A7).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use lpa_agent::{AgentEvent, AgentSession, AppToolset, ModelProvider};

use crate::app::agent::agent_transcript_mirror::AgentTranscriptMirror;
use crate::app::agent::app_agent_host_bridge::{AppAgentBridgeState, AppAgentHostBridge};

/// The concrete `lpa-agent` session the app chat drives.
pub type AppAgentRuntime = AgentSession<Box<dyn ModelProvider>, AppToolset<AppAgentHostBridge>>;

/// The app chat: view mirror + parked session runtime.
pub struct AppAgentSession {
    /// The transcript the view renders.
    pub mirror: AgentTranscriptMirror,
    /// True from run start until `AppRunEnded` arrives.
    pub running: bool,
    /// The snapshot the host bridge serves.
    pub bridge: Rc<RefCell<AppAgentBridgeState>>,
    /// The parked session runtime (`None` while a run future owns it).
    pub runtime: Rc<RefCell<Option<AppAgentRuntime>>>,
    /// The running session's abort flag (Stop).
    pub abort: Arc<AtomicBool>,
}

impl Default for AppAgentSession {
    fn default() -> Self {
        Self {
            mirror: AgentTranscriptMirror::default(),
            running: false,
            bridge: Rc::new(RefCell::new(AppAgentBridgeState::default())),
            runtime: Rc::new(RefCell::new(None)),
            abort: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl AppAgentSession {
    /// Fold one streamed event into the mirror.
    pub fn apply_event(&mut self, event: AgentEvent) {
        self.mirror.apply_event(event);
    }

    /// The run future finished; settle the terminal status.
    pub fn run_ended(&mut self, error: Option<String>) {
        self.running = false;
        self.mirror.run_ended(error);
    }
}
