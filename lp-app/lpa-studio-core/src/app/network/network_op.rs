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
    /// `NetworkSet`: `None` leaves a setting as it is.
    Set {
        ssid: Option<String>,
        password: PasswordChange,
        enabled: Option<bool>,
        lan_only: Option<bool>,
    },
    /// `NetworkForget`: the board drops the saved network and its password;
    /// `lanOnly` stays.
    Forget,
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
            NetworkChange::Set {
                ssid: Some(_),
                password,
                ..
            } if *password != PasswordChange::Keep => ActionMeta::new(
                "Save",
                "Save this network on the board. The password can't be read back.",
                ActionPriority::Primary,
            )
            .with_icon("save"),
            NetworkChange::Set { .. } => ActionMeta::new(
                "Save",
                "Save the board's network settings.",
                ActionPriority::Primary,
            )
            .with_icon("save"),
            // The board forgets a password Studio never kept: only the user
            // can bring it back.
            NetworkChange::Forget => ActionMeta::new(
                "Forget",
                "The board forgets this network and its password.",
                ActionPriority::Tertiary,
            )
            .with_icon("remove")
            .lasting(ActionConfirmation::new(
                "Forget this network?",
                "The board forgets the network and its password. You'll need the password \
                 to set it again.",
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
    fn forget_is_lasting_and_a_set_is_routine() {
        let forget = NetworkOp::action_for(DeviceId(1), NetworkChange::Forget);
        assert!(forget.meta().consequence.arms());
        assert!(forget.meta().needs_user());
        let set = NetworkOp::action_for(
            DeviceId(1),
            NetworkChange::Set {
                ssid: None,
                password: PasswordChange::Keep,
                enabled: Some(false),
                lan_only: None,
            },
        );
        assert!(set.meta().consequence.is_routine());
    }

    /// The op as the recorder and every log see it carries no password.
    #[test]
    fn an_op_never_prints_its_password() {
        let set = NetworkOp::action_for(
            DeviceId(1),
            NetworkChange::Set {
                ssid: Some("lp-walk-net".to_string()),
                password: PasswordChange::Set("correct-horse-42".to_string()),
                enabled: None,
                lan_only: None,
            },
        );
        let printed = format!("{set:?}");
        assert!(printed.contains("lp-walk-net"), "{printed}");
        assert!(!printed.contains("correct-horse-42"), "{printed}");
        let summary = crate::app::studio::studio_command_summary::summarize_command(
            &crate::StudioCommand::Action(set),
        )
        .unwrap();
        assert!(!summary.1.contains("correct-horse-42"), "{summary:?}");
        assert_eq!(summary.0, "Action/studio|network/NetworkOp");
    }
}
