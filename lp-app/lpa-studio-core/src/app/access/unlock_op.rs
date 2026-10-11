//! [`UnlockOp`]: unlock a board, because someone asked — the offer at
//! `devices/<board>/unlock` ([`super::unlock_offer`]).
//!
//! Built only by that offer's binder; the web never builds one. The studio
//! controller runs it by handing the access controller the command it
//! stands for: [`UnlockOp::Ask`] is the card's "Unlock" (raise the sheet),
//! [`UnlockOp::Submit`] is the sheet's submit (try this password on the
//! board's link).

use core::any::Any;

use lpa_devices::DeviceId;

use super::access_command::AccessCommand;
use super::unlock_password::UnlockPassword;
use crate::{ActionClass, ActionMeta, ActionPriority, ControllerId, ControllerOp, UiAction};

/// Unlock `device`.
///
/// `Debug` is derived, and safe: the only secret inside is an
/// [`UnlockPassword`], whose `Debug` never prints the password — so the
/// session recorder's `fmt_op` and any log of the action carry
/// `password: <redacted>`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UnlockOp {
    /// No password yet: raise the Unlock sheet, which asks for one. What an
    /// agent's press of the offer does, so the sheet reaches the person.
    Ask { device: DeviceId },
    /// Try this password on the board's link; `remember` keeps it on this
    /// browser for the next board that takes it.
    Submit {
        device: DeviceId,
        password: UnlockPassword,
        remember: bool,
    },
}

impl UnlockOp {
    /// Routed by `StudioController` to its access controller.
    pub const NODE_ID: &'static str = "studio|access";

    /// This unlock as a dispatchable [`UiAction`].
    pub fn action_for(op: Self) -> UiAction {
        UiAction::from_op(ControllerId::new(Self::NODE_ID), op)
    }

    /// The access command this op stands for.
    pub(crate) fn into_access_command(self) -> AccessCommand {
        match self {
            Self::Ask { device } => AccessCommand::LogIn { device },
            Self::Submit {
                device,
                password,
                remember,
            } => AccessCommand::SubmitPassword {
                device,
                password: password.into_string(),
                remember,
            },
        }
    }
}

impl ControllerOp for UnlockOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Unlock",
            "Enter this board's password.",
            ActionPriority::Primary,
        )
        .with_icon("lock")
    }

    /// A user's unlock: it starts a conversation on the board's link and
    /// returns; the answer comes back on the actor's queue.
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

    const PASSWORD: &str = "correct-horse-42";

    #[test]
    fn an_op_never_prints_its_password() {
        let op = UnlockOp::Submit {
            device: DeviceId(7),
            password: UnlockPassword::new(PASSWORD),
            remember: true,
        };
        assert_eq!(
            format!("{op:?}"),
            "Submit { device: DeviceId(7), password: <redacted>, remember: true }"
        );
        let action = UnlockOp::action_for(op);
        assert!(!format!("{action:?}").contains(PASSWORD));
        // The recorder's own two readings: the action's name, and the
        // command's detail line.
        assert_eq!(
            crate::app::studio::studio_command_summary::action_name(&action),
            "studio|access/Submit"
        );
        let (name, detail) = crate::app::studio::studio_command_summary::summarize_command(
            &crate::StudioCommand::Action(action),
        )
        .unwrap();
        assert_eq!(name, "Action/studio|access/Submit");
        assert!(detail.contains("<redacted>"), "{detail}");
        assert!(!detail.contains(PASSWORD), "{detail}");
    }

    #[test]
    fn each_op_stands_for_the_access_command_it_replaced() {
        let ask = UnlockOp::Ask {
            device: DeviceId(3),
        };
        assert!(matches!(
            ask.into_access_command(),
            AccessCommand::LogIn { device } if device == DeviceId(3)
        ));
        let submit = UnlockOp::Submit {
            device: DeviceId(3),
            password: UnlockPassword::new(PASSWORD),
            remember: false,
        };
        match submit.into_access_command() {
            AccessCommand::SubmitPassword {
                device,
                password,
                remember,
            } => {
                assert_eq!(device, DeviceId(3));
                assert_eq!(password, PASSWORD);
                assert!(!remember);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unlocking_is_routine_and_primary() {
        let action = UnlockOp::action_for(UnlockOp::Ask {
            device: DeviceId(1),
        });
        assert!(action.meta().consequence.is_routine());
        assert!(!action.meta().needs_user());
        assert_eq!(action.meta().label, "Unlock");
        assert_eq!(action.meta().priority, ActionPriority::Primary);
        assert_eq!(action.meta().icon.as_deref(), Some("lock"));
    }
}
