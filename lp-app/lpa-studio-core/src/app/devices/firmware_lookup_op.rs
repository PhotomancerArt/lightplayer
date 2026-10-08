//! "Look up <version>": ask the firmware store for a release by its exact
//! version, when "Other version…"'s box names one the release index does
//! not list ([`super::store_lookups`]).
//!
//! Not a model action — the roster has no opinion on what the store holds
//! — so it is not a [`DevicesOp`](super::DevicesOp). It is the install
//! offer's own binding while the box names an unlisted version, so the
//! person and the app agent press it the same way; once the store answers,
//! the version joins the list and the same press installs it.

use core::any::Any;

use crate::{ActionClass, ActionMeta, ActionPriority, ControllerOp};

/// Ask the store for release `version` of `target`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwareLookupOp {
    pub target: String,
    pub version: String,
}

impl FirmwareLookupOp {
    /// Routed by `StudioController` directly, like the roster's own ops.
    pub const NODE_ID: &'static str = "studio|firmware-lookup";

    /// This look-up as a dispatchable [`UiAction`](crate::UiAction).
    pub fn action_for(target: &str, version: &str) -> crate::UiAction {
        crate::UiAction::from_op(
            crate::ControllerId::new(Self::NODE_ID),
            Self {
                target: target.to_string(),
                version: version.to_string(),
            },
        )
    }
}

impl ControllerOp for FirmwareLookupOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            format!("Look up {}", self.version),
            format!(
                "Ask the firmware store for {}: the list shows only its newest releases.",
                self.version
            ),
            ActionPriority::Secondary,
        )
    }

    /// Asking is bookkeeping (the store answers later): it never cancels a
    /// passive pull or waits behind one.
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
