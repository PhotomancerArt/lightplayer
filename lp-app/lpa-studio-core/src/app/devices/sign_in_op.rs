//! "Sign in": the lightplayer.app row's fix on a card while nobody is
//! signed in (`devices/<board>/sign-in`, [`super::sign_in_offer`]).
//!
//! Not a device action, and core does not know the cloud: the controller
//! counts the request ([`crate::UiStudioView::sign_in_requests`]) and the web
//! shell, which holds the cloud session and its sign-in options, leaves for
//! sign-in when it sees the count advance — the way a backup download goes
//! out. It needs the person's own click: signing in leaves the page and
//! asks for their account, so the app agent hands it to them.

use core::any::Any;

use crate::{ActionClass, ActionMeta, ActionPriority, ControllerOp};

/// Ask the shell to sign in.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SignInOp;

impl SignInOp {
    /// Routed by `StudioController` directly, like the roster's own ops.
    pub const NODE_ID: &'static str = "studio|sign-in";

    /// Signing in, as a dispatchable [`UiAction`](crate::UiAction).
    pub fn action() -> crate::UiAction {
        crate::UiAction::from_op(crate::ControllerId::new(Self::NODE_ID), Self)
    }
}

impl ControllerOp for SignInOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Sign in",
            "Sign in to reach this board from any browser, through lightplayer.app.",
            ActionPriority::Secondary,
        )
        .with_icon("login")
        // It leaves the page for the person's own account.
        .needs_user_activation()
    }

    /// Asking changes nothing on any board: it must never cancel a pull.
    fn action_class(&self) -> ActionClass {
        ActionClass::Passive {
            deadline: crate::PASSIVE_REFRESH_DEADLINE,
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
