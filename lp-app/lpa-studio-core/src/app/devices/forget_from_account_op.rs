//! [`ForgetFromAccountOp`]: take a board off the signed-in account's list,
//! on every computer — "Remove from your account" in the card's danger
//! zone (plan `lp2025/2026-10-19-2305-account-board-list`, D4, D10).
//!
//! Built only by its offer (`devices/<board>/forget-from-account`,
//! [`super::forget_from_account_offer`]); the web never builds one. The
//! studio controller runs it: the board leaves the list here at once, and
//! the forget goes to lightplayer.app through the web edge
//! ([`crate::AccountBoardRequest::Forget`]). It removes no key from the
//! board: that is the Access row's job.

use core::any::Any;

use lpa_devices::BoardKey;

use crate::{ActionClass, ActionConfirmation, ActionMeta, ActionPriority, ControllerOp};

/// The board to take off the account's list, and its name for the copy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ForgetFromAccountOp {
    /// The board's MAC.
    pub board: BoardKey,
    /// What the card calls it.
    pub title: String,
}

impl ForgetFromAccountOp {
    /// Routed by `StudioController` to the account's list.
    pub const NODE_ID: &'static str = "studio|forget-from-account";

    /// This forget as a dispatchable [`UiAction`](crate::UiAction).
    pub fn action_for(op: Self) -> crate::UiAction {
        crate::UiAction::from_op(crate::ControllerId::new(Self::NODE_ID), op)
    }
}

/// The verb's label.
pub const REMOVE_FROM_YOUR_ACCOUNT: &str = "Remove from your account";

impl ControllerOp for ForgetFromAccountOp {
    fn default_action_meta(&self) -> ActionMeta {
        let leaves = format!(
            "{} leaves your account's list on every computer. The board keeps its keys, \
             and it comes back the next time it reaches lightplayer.app.",
            self.title
        );
        ActionMeta::new(
            REMOVE_FROM_YOUR_ACCOUNT,
            leaves.clone(),
            ActionPriority::Tertiary,
        )
        .with_icon("remove")
        .lasting(ActionConfirmation::new(
            "Remove this board from your account?",
            leaves,
            "Remove",
        ))
    }

    /// A press that changes core's list at once; the cloud's answer comes
    /// back on the actor's queue.
    fn action_class(&self) -> ActionClass {
        ActionClass::Foreground {
            deadline: crate::PROJECT_ACTION_DEADLINE,
        }
    }

    fn clone_box(&self) -> Box<dyn ControllerOp> {
        Box::new(self.clone())
    }

    fn eq_op(&self, other: &dyn ControllerOp) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_is_lasting_and_says_what_is_lost_and_what_is_kept() {
        let action = ForgetFromAccountOp::action_for(ForgetFromAccountOp {
            board: BoardKey::parse("10bda3b08e30").unwrap(),
            title: "Porch".to_string(),
        });
        let meta = action.meta();
        assert_eq!(meta.label, REMOVE_FROM_YOUR_ACCOUNT);
        assert!(
            meta.consequence.arms(),
            "two clicks; the agent never presses it"
        );
        assert_eq!(
            meta.summary,
            "Porch leaves your account's list on every computer. The board keeps its keys, \
             and it comes back the next time it reaches lightplayer.app."
        );
    }
}
