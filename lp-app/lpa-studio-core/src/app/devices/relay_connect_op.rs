//! [`RelayConnectOp`]: reach a known board through lightplayer.app's relay
//! because someone asked — "Connect through lightplayer.app" on the tile of
//! a board Studio has met, from any network.
//!
//! Built only by its offer (`devices/<board>/connect-relay`,
//! [`super::relay_connect_offer`]); the web never builds one. The studio
//! controller runs it: it opens the board's session through the relay
//! transport and says how it went in plain words
//! ([`super::relay_connect_failure`]).

use core::any::Any;

use lpa_devices::DeviceId;

use crate::{ActionClass, ActionMeta, ActionPriority, ControllerOp};

/// The board to reach through the relay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayConnectOp {
    pub device: DeviceId,
}

impl RelayConnectOp {
    /// Routed by `StudioController` to its relay connect.
    pub const NODE_ID: &'static str = "studio|relay-connect";

    /// This connect as a dispatchable [`UiAction`](crate::UiAction).
    pub fn action_for(op: Self) -> crate::UiAction {
        crate::UiAction::from_op(crate::ControllerId::new(Self::NODE_ID), op)
    }
}

impl ControllerOp for RelayConnectOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Connect through lightplayer.app",
            "Reach this board from anywhere, through lightplayer.app.",
            ActionPriority::Secondary,
        )
        .with_icon("wifi")
    }

    /// A user's connect: it opens a socket and returns; the answer comes
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
    fn the_connect_is_routine_and_names_lightplayer_app() {
        let action = RelayConnectOp::action_for(RelayConnectOp {
            device: DeviceId(1),
        });
        assert_eq!(action.meta().label, "Connect through lightplayer.app");
        assert!(action.meta().consequence.is_routine());
        assert!(!action.meta().needs_user());
    }
}
