//! [`UiAgentCard`]: an action the app agent proposed that only the user may
//! press (plan D6, PD6).
//!
//! The card carries the original [`UiAction`] verbatim as its `press`: the
//! card's button dispatches exactly what the user's own button would, so
//! there is no agent-only path and nothing new for a browser picker to
//! prove — the click is the user's. Pressing that same action from anywhere
//! (the card, or the device card it came from) resolves the card. The model
//! can never mark one done.
//!
//! **A verb that takes values** (flash *which board*) is handed over as the
//! real control, not as a frozen press: the card names the offer's path and
//! carries the agent's values as the pre-selection. A renderer looks the
//! offer up in the live tree, draws its parameters set to those values, and
//! the user may change them before pressing; the press is the offer's own
//! binding of whatever the user settled on. Any press of that offer
//! resolves the card ([`UiAgentCard::answered_by`]), and the agent hears
//! which values the user changed ([`UiAgentCard::resume_text`]).

use crate::app::agent::agent_controller::AgentController;
use crate::app::agent::agent_op::AgentOp;
use crate::{ControllerId, OfferArgs, OfferPath, UiAction};

/// One card in the app chat's transcript.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiAgentCard {
    /// Session-scoped id (`c1`, `c2`, …), what the agent was told.
    pub id: String,
    /// What the button does, in the app's own words (a lasting action's
    /// copy title, or its label).
    pub title: String,
    /// What happens and what is at stake (a lasting action's copy message,
    /// or the action's summary).
    pub message: String,
    /// The assistant's one line on why now. Model text: render as plain
    /// text.
    pub why: String,
    /// The button's label.
    pub confirm_label: String,
    /// The action takes something away (the error tint; see
    /// [`crate::ActionConsequence::wears_error_tint`]).
    pub destructive: bool,
    pub state: UiAgentCardState,
    /// The card's button: the original action, verbatim — for an offer
    /// that takes values, the agent's values bound.
    pub press: UiAction,
    /// The offer the agent pressed, when the card hands one over: a
    /// renderer draws its parameters from the live tree at this path.
    pub offer: Option<OfferPath>,
    /// The agent's values for that offer: the controls' pre-selection.
    pub args: OfferArgs,
    /// The values the user's press carried, once it answered the card
    /// through the offer.
    pub user_args: Option<OfferArgs>,
}

/// Where a card stands.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiAgentCardState {
    /// Waiting for the user.
    Pending,
    /// The user pressed it; what the app said back.
    Done { outcome: String },
    /// The user pressed it and the app refused or failed.
    Failed { error: String },
    /// The user said no.
    Dismissed,
}

impl UiAgentCard {
    /// A pending card for `action`, in the action's own words.
    pub fn new(id: impl Into<String>, action: UiAction, why: impl Into<String>) -> Self {
        let meta = action.meta();
        let (title, message, confirm_label) = match meta.consequence.copy() {
            Some(confirmation) => (
                confirmation.title.clone(),
                confirmation.message.clone(),
                confirmation.confirm_label.clone(),
            ),
            None => (meta.label.clone(), meta.summary.clone(), meta.label.clone()),
        };
        Self {
            id: id.into(),
            title,
            message,
            why: why.into(),
            confirm_label,
            destructive: meta.consequence.wears_error_tint(),
            state: UiAgentCardState::Pending,
            press: action,
            offer: None,
            args: OfferArgs::new(),
            user_args: None,
        }
    }

    /// This card hands over the offer at `path`, pre-filled with `args`.
    pub fn for_offer(mut self, path: OfferPath, args: OfferArgs) -> Self {
        self.offer = Some(path);
        self.args = args;
        self
    }

    /// Whether pressing `action` is pressing this card: the same operation
    /// as its press, or any press of the offer it hands over (the user may
    /// have picked other values).
    pub fn answered_by(&self, action: &UiAction) -> bool {
        self.press.same_op(action)
            || self.offer.as_ref().is_some_and(|path| {
                action
                    .offer_press()
                    .is_some_and(|press| &press.path == path)
            })
    }

    pub fn is_pending(&self) -> bool {
        self.state == UiAgentCardState::Pending
    }

    /// The Dismiss button's action.
    pub fn dismiss_action(&self) -> UiAction {
        UiAction::from_op(
            ControllerId::new(AgentController::NODE_ID),
            AgentOp::CardDismissed {
                card: self.id.clone(),
            },
        )
    }

    /// The line the resumed run reads: what the user did with this card,
    /// and which of the agent's values they changed before pressing.
    pub fn resume_text(&self) -> String {
        let changed = self.changed_values();
        match &self.state {
            UiAgentCardState::Done { outcome } if outcome.is_empty() => {
                format!(
                    "[I clicked \"{}\" on card {}{changed}]",
                    self.title, self.id
                )
            }
            UiAgentCardState::Done { outcome } => format!(
                "[I clicked \"{}\" on card {}{changed}: {outcome}]",
                self.title, self.id
            ),
            UiAgentCardState::Failed { error } => format!(
                "[I clicked \"{}\" on card {}{changed}, and it failed: {error}]",
                self.title, self.id
            ),
            UiAgentCardState::Dismissed => {
                format!("[I dismissed card {} (\"{}\")]", self.id, self.title)
            }
            UiAgentCardState::Pending => String::new(),
        }
    }

    /// The line the resumed run reads when the card's press opened the
    /// browser's picker and the user closed it with nothing picked: the
    /// card is still pending, so the user can press it again.
    pub fn chooser_cancelled_text(&self) -> String {
        format!(
            "[I clicked \"{}\" on card {} but cancelled the browser's picker without \
             picking anything; the card is still there to click again]",
            self.title, self.id
        )
    }

    /// ` with board = b (you chose a)` for each value the user's press set
    /// differently from the agent's; empty when they pressed it as handed
    /// over.
    fn changed_values(&self) -> String {
        let Some(user) = &self.user_args else {
            return String::new();
        };
        let changes: Vec<String> = user
            .iter()
            .filter(|(name, value)| self.args.get(name) != Some(*value))
            .map(|(name, value)| match self.args.get(name) {
                Some(agent) => format!("{name} = {value} (you chose {agent})"),
                None => format!("{name} = {value}"),
            })
            .collect();
        if changes.is_empty() {
            String::new()
        } else {
            format!(" with {}", changes.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActionConfirmation, DevicesOp};

    #[test]
    fn a_card_speaks_in_the_actions_own_words() {
        let usb = DevicesOp::action_for(lpa_devices::Action::AddFromUsb)
            .with_label("Connect a board via USB");
        let card = UiAgentCard::new("c1", usb.clone(), "so I can put your patterns on it");
        assert_eq!(card.title, "Connect a board via USB");
        assert!(card.message.contains("USB port"), "{}", card.message);
        assert_eq!(
            card.press, usb,
            "the press is the original action, verbatim"
        );
        assert!(card.is_pending());

        let forget = DevicesOp::action_for(lpa_devices::Action::Forget {
            device: lpa_devices::DeviceId(3),
        });
        let card = UiAgentCard::new("c2", forget, "you asked to start over");
        let ActionConfirmation {
            title,
            confirm_label,
            ..
        } = card
            .press
            .meta()
            .consequence
            .copy()
            .cloned()
            .expect("Forget is lasting");
        assert_eq!(card.title, title);
        assert_eq!(card.confirm_label, confirm_label);
        assert!(card.destructive);
    }

    #[test]
    fn a_card_for_an_offer_is_answered_by_any_press_of_it() {
        use crate::{OfferPath, ProjectOp};
        let path = OfferPath::parse("devices/mac-a0f26287b48c/flash").unwrap();
        let flash = DevicesOp::action_for(lpa_devices::Action::Flash {
            device: lpa_devices::DeviceId(3),
            board_id: "xiao".to_string(),
            build_id: "b".to_string(),
            park_first: false,
            name: None,
        });
        let card = UiAgentCard::new("c1", flash.clone(), "x")
            .for_offer(path.clone(), OfferArgs::new().with("board", "xiao"));
        assert!(card.answered_by(&flash), "the agent's own press");

        let other_board = DevicesOp::action_for(lpa_devices::Action::Flash {
            device: lpa_devices::DeviceId(3),
            board_id: "devkit".to_string(),
            build_id: "b".to_string(),
            park_first: false,
            name: None,
        });
        assert!(
            !card.answered_by(&other_board),
            "another op, not pressed from the offer"
        );
        let offer = crate::UiOffer::new(path, "flash", other_board);
        let pressed = offer.press(&OfferArgs::new()).unwrap();
        assert!(card.answered_by(&pressed), "a press of the card's offer");

        let save = UiAction::from_op(ControllerId::new("studio|project"), ProjectOp::SaveOverlay);
        assert!(!card.answered_by(&save));
    }

    #[test]
    fn the_resumed_run_hears_which_values_the_user_changed() {
        let mut card = UiAgentCard::new(
            "c1",
            DevicesOp::action_for(lpa_devices::Action::AddFromUsb),
            "x",
        )
        .for_offer(
            crate::OfferPath::parse("devices/new-3/flash").unwrap(),
            OfferArgs::new().with("board", "xiao"),
        );
        card.state = UiAgentCardState::Done {
            outcome: "flashed".to_string(),
        };
        card.user_args = Some(OfferArgs::new().with("board", "xiao"));
        assert_eq!(
            card.resume_text(),
            "[I clicked \"via USB\" on card c1: flashed]",
            "pressed as handed over"
        );
        card.user_args = Some(
            OfferArgs::new()
                .with("board", "devkit")
                .with("name", "Desk"),
        );
        assert_eq!(
            card.resume_text(),
            "[I clicked \"via USB\" on card c1 with board = devkit (you chose xiao), \
             name = Desk: flashed]"
        );
    }

    #[test]
    fn the_resumed_run_hears_what_the_user_did() {
        let mut card = UiAgentCard::new(
            "c1",
            DevicesOp::action_for(lpa_devices::Action::AddFromUsb),
            "x",
        );
        card.state = UiAgentCardState::Done {
            outcome: String::new(),
        };
        assert_eq!(card.resume_text(), "[I clicked \"via USB\" on card c1]");
        card.state = UiAgentCardState::Dismissed;
        assert!(card.resume_text().starts_with("[I dismissed card c1"));
    }
}
