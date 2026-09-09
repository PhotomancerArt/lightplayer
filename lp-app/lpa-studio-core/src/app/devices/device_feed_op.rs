//! The device card's mount lease for its live frame feed.
//!
//! Not a model action — the roster has no opinion on whether a person can
//! see a card — so it is not a [`DevicesOp`](super::DevicesOp). A mounted
//! `DeviceRosterCard` sends `wanted: true`, an unmounting one `false`, and
//! the feed pulls only for wanted cards on a visible page.

use core::any::Any;

use lpa_devices::identity::DeviceId;

use crate::{ActionClass, ActionMeta, ActionPriority, ControllerOp};

/// "A card for this device is (no longer) on screen."
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceFeedOp {
    pub device: DeviceId,
    pub wanted: bool,
}

impl DeviceFeedOp {
    /// The node id the lease targets. Routed by `StudioController`
    /// directly, like the roster's own ops.
    pub const NODE_ID: &'static str = "studio|device-feed";

    /// This lease as a dispatchable [`UiAction`](crate::UiAction).
    pub fn action_for(device: DeviceId, wanted: bool) -> crate::UiAction {
        crate::UiAction::from_op(
            crate::ControllerId::new(Self::NODE_ID),
            Self { device, wanted },
        )
    }
}

impl ControllerOp for DeviceFeedOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Watch",
            "Keep this board's picture live while its card is on screen.",
            ActionPriority::Tertiary,
        )
    }

    /// A lease is bookkeeping, not a gesture: it must never cancel a
    /// passive pull or wait behind one.
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
