//! [`NetworkOp`]: one change to a board's Wi‑Fi settings, as the offers at
//! `devices/<board>/wifi/…` dispatch it.

use core::any::Any;

use lpa_devices::DeviceId;

use super::wifi_password_change::PasswordChange;
use crate::{ActionClass, ActionConfirmation, ActionMeta, ActionPriority, ControllerOp};

/// One change to `device`'s network settings. Built only by the Wi‑Fi
/// offers' binders ([`super::wifi_offers`]); the web never builds one.
///
/// `Debug` is derived, and safe: the only secret inside is a
/// [`PasswordChange`], whose `Debug` never prints the password — so the
/// session recorder's `fmt_op` and any log of the action carry
/// `Set(<redacted>)`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkOp {
    pub device: DeviceId,
    pub change: NetworkChange,
}

/// What a [`NetworkOp`] changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NetworkChange {
    /// `NetworkAdd`: save a network, or give a saved one a new password.
    /// `hidden: None` leaves a saved network's as it is.
    Add {
        ssid: String,
        password: PasswordChange,
        hidden: Option<bool>,
    },
    /// `NetworkForget`: the board drops this network and its password.
    Forget { ssid: String },
    /// `NetworkSet`: the two switches; `None` leaves one as it is.
    Switches {
        wifi: Option<bool>,
        cloud_relay: Option<bool>,
    },
}

impl NetworkOp {
    /// Routed by `StudioController` to its network controller.
    pub const NODE_ID: &'static str = "studio|network";

    /// This change as a dispatchable [`UiAction`](crate::UiAction).
    pub fn action_for(device: DeviceId, change: NetworkChange) -> crate::UiAction {
        crate::UiAction::from_op(
            crate::ControllerId::new(Self::NODE_ID),
            Self { device, change },
        )
    }
}

impl ControllerOp for NetworkOp {
    fn default_action_meta(&self) -> ActionMeta {
        match &self.change {
            NetworkChange::Add { .. } => ActionMeta::new(
                "Connect",
                "Save this network on the board, then connect to it. The password can't be \
                 read back.",
                ActionPriority::Primary,
            )
            .with_icon("wifi"),
            NetworkChange::Switches { .. } => ActionMeta::new(
                "Save",
                "Save the board's Wi‑Fi switches.",
                ActionPriority::Secondary,
            )
            .with_icon("save"),
            // The board forgets a password Studio never kept: only the user
            // can bring it back.
            NetworkChange::Forget { ssid } => ActionMeta::new(
                "Forget",
                "The board forgets this network and its password.",
                ActionPriority::Tertiary,
            )
            .with_icon("remove")
            .lasting(ActionConfirmation::new(
                format!("Forget {ssid}?"),
                "The board forgets it and its password. You'll need the password to add it \
                 again.",
                "Forget",
            )),
        }
    }

    /// A user's change: it should not wait behind a background pull, and
    /// it only starts a conversation on the board's shared wire.
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
    fn forget_is_lasting_and_the_switches_are_routine() {
        let forget = NetworkOp::action_for(
            DeviceId(1),
            NetworkChange::Forget {
                ssid: "lp-walk-net".to_string(),
            },
        );
        assert!(forget.meta().consequence.arms());
        assert!(forget.meta().needs_user());
        let switches = NetworkOp::action_for(
            DeviceId(1),
            NetworkChange::Switches {
                wifi: Some(false),
                cloud_relay: None,
            },
        );
        assert!(switches.meta().consequence.is_routine());
    }

    /// The op as the recorder and every log see it carries no password.
    #[test]
    fn an_op_never_prints_its_password() {
        let add = NetworkOp::action_for(
            DeviceId(1),
            NetworkChange::Add {
                ssid: "lp-walk-net".to_string(),
                password: PasswordChange::Set("correct-horse-42".to_string()),
                hidden: None,
            },
        );
        let printed = format!("{add:?}");
        assert!(printed.contains("lp-walk-net"), "{printed}");
        assert!(!printed.contains("correct-horse-42"), "{printed}");
        let summary = crate::app::studio::studio_command_summary::summarize_command(
            &crate::StudioCommand::Action(add),
        )
        .unwrap();
        assert!(!summary.1.contains("correct-horse-42"), "{summary:?}");
        assert_eq!(summary.0, "Action/studio|network/NetworkOp");
    }
}
