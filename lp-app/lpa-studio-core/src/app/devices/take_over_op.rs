//! [`TakeOverOp`]: connect to a board another Studio tab of this browser
//! holds, by asking that tab to let it go first.
//!
//! Built only by its offer (`devices/<board>/take-over`,
//! [`super::take_over_offer`]); the web never builds one. The studio
//! controller runs it: it asks the holder on the hold channel, waits for
//! the answer, and opens the board here once the holder has let go — or
//! says why not ([`super::take_over_state`]).

use core::any::Any;

use lpa_devices::DeviceId;

use crate::{ActionClass, ActionMeta, ActionPriority, ControllerOp};

/// The board to take over.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TakeOverOp {
    pub device: DeviceId,
}

impl TakeOverOp {
    /// Routed by `StudioController` to its take-over.
    pub const NODE_ID: &'static str = "studio|take-over";

    /// This take-over as a dispatchable [`UiAction`](crate::UiAction).
    pub fn action_for(op: Self) -> crate::UiAction {
        crate::UiAction::from_op(crate::ControllerId::new(Self::NODE_ID), op)
    }
}

impl ControllerOp for TakeOverOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Connect",
            "Ask your other tab to let go of this board, then connect here.",
            ActionPriority::Primary,
        )
        .with_icon("connect")
    }

    /// A user's connect: it posts the ask and returns; the answer comes
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
    fn the_take_over_is_a_connect_that_asks_first() {
        let action = TakeOverOp::action_for(TakeOverOp {
            device: DeviceId(1),
        });
        assert_eq!(action.meta().label, "Connect");
        assert!(action.meta().summary.contains("other tab"));
        assert!(!action.meta().needs_user());
    }
}
