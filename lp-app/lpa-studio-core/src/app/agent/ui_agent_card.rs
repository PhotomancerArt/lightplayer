//! [`UiAgentCard`]: an action the app agent proposed that only the user may
//! press (plan D6, PD6).
//!
//! The card carries the original [`UiAction`] verbatim as its `press`: the
//! card's button dispatches exactly what the user's own button would, so
//! there is no agent-only path and nothing new for a browser picker to
//! prove — the click is the user's. Pressing that same action from anywhere
//! (the card, or the device card it came from) resolves the card. The model
//! can never mark one done.

use crate::app::agent::agent_controller::AgentController;
use crate::app::agent::agent_op::AgentOp;
use crate::{ControllerId, UiAction};

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
    /// The card's button: the original action, verbatim.
    pub press: UiAction,
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
        }
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

    /// The line the resumed run reads: what the user did with this card.
    pub fn resume_text(&self) -> String {
        match &self.state {
            UiAgentCardState::Done { outcome } if outcome.is_empty() => {
                format!("[I clicked \"{}\" on card {}]", self.title, self.id)
            }
            UiAgentCardState::Done { outcome } => format!(
                "[I clicked \"{}\" on card {}: {outcome}]",
                self.title, self.id
            ),
            UiAgentCardState::Failed { error } => format!(
                "[I clicked \"{}\" on card {}, and it failed: {error}]",
                self.title, self.id
            ),
            UiAgentCardState::Dismissed => {
                format!("[I dismissed card {} (\"{}\")]", self.id, self.title)
            }
            UiAgentCardState::Pending => String::new(),
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
