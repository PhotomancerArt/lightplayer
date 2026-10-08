//! [`WifiConnectOp`]: reach a board on the LAN because someone asked —
//! "Connect over Wi‑Fi" on a known board's card, or an address typed into
//! "Connect a board on Wi‑Fi" under Connect a device.
//!
//! Built only by the offers (`devices/<board>/connect-wifi`,
//! `devices/connect-wifi-address`); the web never builds one. The studio
//! controller runs it: it opens the board's session through the LAN
//! transport and says how it went in plain words
//! ([`super::wifi_connect_failure`]).

use core::any::Any;

use lpa_devices::DeviceId;

use crate::{ActionClass, ActionMeta, ActionPriority, ControllerOp};

/// Who to reach over Wi‑Fi.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WifiConnectOp {
    /// A known board, at the address this browser remembers for it.
    Board { device: DeviceId },
    /// Whatever board answers at `url` (`ws://<host>/link`, normalised).
    Address { url: String },
}

impl WifiConnectOp {
    /// Routed by `StudioController` to its Wi‑Fi connect.
    pub const NODE_ID: &'static str = "studio|wifi-connect";

    /// This connect as a dispatchable [`UiAction`](crate::UiAction).
    pub fn action_for(op: Self) -> crate::UiAction {
        crate::UiAction::from_op(crate::ControllerId::new(Self::NODE_ID), op)
    }
}

impl ControllerOp for WifiConnectOp {
    fn default_action_meta(&self) -> ActionMeta {
        match self {
            Self::Board { .. } => ActionMeta::new(
                "Connect over Wi\u{2011}Fi",
                "Reach this board on your network, at the address it last gave.",
                ActionPriority::Secondary,
            )
            .with_icon("wifi"),
            Self::Address { .. } => ActionMeta::new(
                "Connect a board on Wi\u{2011}Fi",
                "Reach the board at this address on your network.",
                ActionPriority::Secondary,
            )
            .with_icon("wifi"),
        }
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
    fn both_connects_are_routine_and_name_wifi() {
        let card = WifiConnectOp::action_for(WifiConnectOp::Board {
            device: DeviceId(1),
        });
        assert_eq!(card.meta().label, "Connect over Wi\u{2011}Fi");
        assert!(card.meta().consequence.is_routine());
        let typed = WifiConnectOp::action_for(WifiConnectOp::Address {
            url: "ws://10.0.0.5/link".to_string(),
        });
        assert!(typed.meta().consequence.is_routine());
        assert!(!typed.meta().needs_user());
    }
}
