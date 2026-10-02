//! [`AppAgentSession`]: the one app-level agent conversation a page holds
//! (in memory for the page's life — plan A7).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use lpa_agent::{AgentEvent, AgentSession, AppToolset, ModelProvider};

use crate::UiAgentTurn;
use crate::app::agent::agent_transcript_mirror::AgentTranscriptMirror;
use crate::app::agent::app_agent_host_bridge::{AppAgentBridgeState, AppAgentHostBridge};
use crate::app::agent::ui_agent_card::{UiAgentCard, UiAgentCardState};

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
    /// Cards minted so far (ids are `c1`, `c2`, … for the page's life).
    pub cards_minted: u32,
    /// What the user did with a card while a run was still out: the run
    /// that follows it hears it.
    pub resume: Vec<String>,
}

impl Default for AppAgentSession {
    fn default() -> Self {
        Self {
            mirror: AgentTranscriptMirror::default(),
            running: false,
            bridge: Rc::new(RefCell::new(AppAgentBridgeState::default())),
            runtime: Rc::new(RefCell::new(None)),
            abort: Arc::new(AtomicBool::new(false)),
            cards_minted: 0,
            resume: Vec::new(),
        }
    }
}

impl AppAgentSession {
    /// Fold one streamed event into the mirror.
    pub fn apply_event(&mut self, event: AgentEvent) {
        self.mirror.apply_event(event);
    }

    /// Put a pending card for `action` in the transcript; its id.
    pub fn add_card(&mut self, action: crate::UiAction, why: &str) -> UiAgentCard {
        self.cards_minted += 1;
        let card = UiAgentCard::new(format!("c{}", self.cards_minted), action, why);
        self.mirror.turns.push(UiAgentTurn::Card(card.clone()));
        card
    }

    /// The pending card, if one is waiting for the user.
    pub fn pending_card(&self) -> Option<&UiAgentCard> {
        self.cards().find(|card| card.is_pending())
    }

    /// The pending card `action` presses, if any.
    pub fn pending_card_for(&self, action: &crate::UiAction) -> Option<String> {
        self.cards()
            .find(|card| card.is_pending() && card.press.same_op(action))
            .map(|card| card.id.clone())
    }

    /// Settle card `id` and queue what the user did for the assistant.
    /// `false` when no such pending card exists.
    pub fn settle_card(&mut self, id: &str, state: UiAgentCardState) -> bool {
        let Some(card) = self.mirror.turns.iter_mut().find_map(|turn| match turn {
            UiAgentTurn::Card(card) if card.id == id && card.is_pending() => Some(card),
            _ => None,
        }) else {
            return false;
        };
        card.state = state;
        self.resume.push(card.resume_text());
        true
    }

    fn cards(&self) -> impl Iterator<Item = &UiAgentCard> {
        self.mirror.turns.iter().filter_map(|turn| match turn {
            UiAgentTurn::Card(card) => Some(card),
            _ => None,
        })
    }

    /// The run future finished; settle the terminal status.
    pub fn run_ended(&mut self, error: Option<String>) {
        self.running = false;
        self.mirror.run_ended(error);
    }
}
