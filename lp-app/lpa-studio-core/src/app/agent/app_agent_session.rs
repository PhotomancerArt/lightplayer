//! [`AppAgentSession`]: the one app-level agent conversation a page holds
//! (in memory for the page's life — plan A7).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use lpa_agent::{AgentEvent, AgentSession, AppToolset, ModelProvider};
use lpa_devices::GrantAnswer;

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
    /// A card whose press opened a platform chooser: it settles on the
    /// chooser's answer, not on the press (which only opened it).
    pub grant_wait: Option<CardGrantWait>,
    /// An open from Home the agent started that is still under way: it
    /// lands (or fails) after the run that pressed it has ended, and the
    /// run that follows hears how it went.
    pub open_wait: Option<AgentOpenWait>,
    /// How the agent's open ended, waiting for a run to hear it: the next
    /// resume carries it, unless a card is waiting for the user — then the
    /// card's settle carries both.
    pub open_settled: Option<String>,
}

/// An open from Home the agent pressed, by the key the open is pending
/// under (the library uid of the project being opened).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentOpenWait {
    pub key: String,
}

/// A card waiting on the chooser its press opened.
#[derive(Clone, Debug)]
pub struct CardGrantWait {
    pub card: String,
    /// The link id the chooser answers for
    /// ([`lpa_devices::Event::GrantAnswered`]).
    pub link: crate::DeviceLinkId,
    /// Where the user's press came from (see [`AppAgentSession::settle_card`]).
    pub press: Option<crate::OfferPress>,
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
            grant_wait: None,
            open_wait: None,
            open_settled: None,
        }
    }
}

impl AppAgentSession {
    /// Fold one streamed event into the mirror.
    pub fn apply_event(&mut self, event: AgentEvent) {
        self.mirror.apply_event(event);
    }

    /// Put a pending card for `action` in the transcript — the press of
    /// the offer at `offer` with the agent's `args` — and return it.
    pub fn add_card(
        &mut self,
        action: crate::UiAction,
        why: &str,
        offer: crate::OfferPath,
        args: crate::OfferArgs,
    ) -> UiAgentCard {
        self.cards_minted += 1;
        let card =
            UiAgentCard::new(format!("c{}", self.cards_minted), action, why).for_offer(offer, args);
        self.mirror.turns.push(UiAgentTurn::Card(card.clone()));
        card
    }

    /// The pending card, if one is waiting for the user.
    pub fn pending_card(&self) -> Option<&UiAgentCard> {
        self.cards().find(|card| card.is_pending())
    }

    /// The pending card `action` presses, if any: the same operation as
    /// the card's press, or any press of the offer the card hands over
    /// ([`UiAgentCard::answered_by`]).
    pub fn pending_card_for(&self, action: &crate::UiAction) -> Option<String> {
        self.cards()
            .find(|card| card.is_pending() && card.answered_by(action))
            .map(|card| card.id.clone())
    }

    /// Settle card `id` and queue what the user did for the assistant;
    /// `press` is where the user's press came from, so a press of the
    /// card's own offer tells the agent which values the user settled on.
    /// `false` when no such pending card exists.
    pub fn settle_card(
        &mut self,
        id: &str,
        state: UiAgentCardState,
        press: Option<crate::OfferPress>,
    ) -> bool {
        let Some(card) = self.mirror.turns.iter_mut().find_map(|turn| match turn {
            UiAgentTurn::Card(card) if card.id == id && card.is_pending() => Some(card),
            _ => None,
        }) else {
            return false;
        };
        card.state = state;
        card.user_args = press
            .filter(|press| card.offer.as_ref() == Some(&press.path))
            .map(|press| press.args);
        self.resume.push(card.resume_text());
        true
    }

    /// The chooser that answers for `link` came back: settle the card that
    /// waits on it. A device picked is Done; a refusal is Failed; a chooser
    /// closed with nothing picked leaves the card pending — the user can
    /// press it again — and the assistant hears that the picker was
    /// cancelled. `false` when no pending card waits on `link`.
    pub fn grant_answered(&mut self, link: crate::DeviceLinkId, answer: &GrantAnswer) -> bool {
        let Some(wait) = self.grant_wait.take_if(|wait| wait.link == link) else {
            return false;
        };
        match answer {
            GrantAnswer::Picked => self.settle_card(
                &wait.card,
                UiAgentCardState::Done {
                    outcome: "a board was picked".to_string(),
                },
                wait.press,
            ),
            GrantAnswer::Failed { error } => self.settle_card(
                &wait.card,
                UiAgentCardState::Failed {
                    error: error.clone(),
                },
                wait.press,
            ),
            GrantAnswer::Dismissed => {
                let Some(card) = self
                    .cards()
                    .find(|card| card.id == wait.card && card.is_pending())
                else {
                    return false;
                };
                let line = card.chooser_cancelled_text();
                self.resume.push(line);
                true
            }
        }
    }

    /// The agent pressed an open that is still under way (`key` is what
    /// the open is pending under). One wait at a time: a newer open
    /// replaces it.
    pub fn agent_open_started(&mut self, key: String) {
        self.open_wait = Some(AgentOpenWait { key });
        self.open_settled = None;
    }

    /// The agent's open ended: queue `note` for the run that follows.
    /// `false` when the agent was not waiting on an open (so one open
    /// resumes the agent at most once).
    pub fn agent_open_settled(&mut self, note: String) -> bool {
        if self.open_wait.take().is_none() {
            return false;
        }
        self.open_settled = Some(note);
        true
    }

    /// The agent's open was superseded by one the user started, or the
    /// user spoke or stopped the agent: nobody is owed a resume for it.
    pub fn forget_agent_open(&mut self) {
        self.open_wait = None;
        self.open_settled = None;
    }

    /// Everything the next resumed run should hear, taken: what the user
    /// did with cards, then how the agent's open ended — the latter held
    /// back while a card is still waiting for the user (the card's settle
    /// carries it). Empty when nothing is owed.
    pub fn take_resume(&mut self) -> Vec<String> {
        let mut lines = std::mem::take(&mut self.resume);
        if self.pending_card().is_none()
            && let Some(note) = self.open_settled.take()
        {
            lines.push(note);
        }
        lines
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

#[cfg(test)]
mod tests {
    use super::*;

    /// One open resumes the agent once; a second settle of the same open
    /// is not owed anything.
    #[test]
    fn an_open_settles_once() {
        let mut session = AppAgentSession::default();
        assert!(!session.agent_open_settled("[late]".into()), "no open");
        session.agent_open_started("prj1".into());
        assert!(session.agent_open_settled("[open]".into()));
        assert!(!session.agent_open_settled("[again]".into()));
        assert_eq!(session.take_resume(), ["[open]"]);
        assert!(session.take_resume().is_empty());
    }

    /// While a card waits for the user, the open's note waits with it and
    /// rides the run the card's settle starts, after what the user did.
    #[test]
    fn a_pending_card_holds_the_open_note_until_it_settles() {
        let mut session = AppAgentSession::default();
        session.agent_open_started("prj1".into());
        // Any action will do: the card's press is not what this is about.
        let new = crate::app::home::new_project_offer(true)
            .press(&crate::OfferArgs::new())
            .expect("project/new presses with its defaults");
        let card = session.add_card(
            new,
            "so there is a project to build in",
            crate::OfferPath::project().child("new"),
            crate::OfferArgs::new(),
        );
        assert!(session.agent_open_settled("[open]".into()));
        assert!(session.take_resume().is_empty(), "the card is waiting");
        assert!(session.settle_card(&card.id, UiAgentCardState::Dismissed, None));
        let resume = session.take_resume();
        assert_eq!(resume.len(), 2, "{resume:?}");
        assert_eq!(resume[1], "[open]");
    }

    /// The user spoke (or stopped the agent): nothing is owed.
    #[test]
    fn a_forgotten_open_owes_nothing() {
        let mut session = AppAgentSession::default();
        session.agent_open_started("prj1".into());
        session.forget_agent_open();
        assert!(!session.agent_open_settled("[open]".into()));
        assert!(session.take_resume().is_empty());
    }
}
