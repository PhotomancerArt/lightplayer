//! The Play surface's mount lease (M5, "keep Play over BLE lean").
//!
//! Play mode is presentational — the core never learned the route — but it
//! is the one mode with an idle read budget over Bluetooth: a phone sitting
//! on a piece's panel must not keep a live-read stream on a link whose air
//! time the board shares with ESP-NOW. So the mounted `PlayModeSurface`
//! says so, the way a device card leases its frame feed: `shown: true` on
//! mount, `false` on drop. The controller counts leases, and the lens gap
//! policy reads the count (`refresh_cadence::lens_refresh_gap_policy`).
//!
//! Not a model action and not a gesture: it never preempts a pull.

use core::any::Any;

use crate::{ActionClass, ActionMeta, ActionPriority, ControllerOp};

/// "A Play surface is (no longer) showing the lens."
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayViewOp {
    pub shown: bool,
}

impl PlayViewOp {
    /// The node id the lease targets, routed by `StudioController`.
    pub const NODE_ID: &'static str = "studio|play-view";

    /// This lease as a dispatchable [`UiAction`](crate::UiAction).
    pub fn action_for(shown: bool) -> crate::UiAction {
        crate::UiAction::from_op(crate::ControllerId::new(Self::NODE_ID), Self { shown })
    }
}

impl ControllerOp for PlayViewOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Play",
            "Keep reads light while the Play panel is on screen.",
            ActionPriority::Tertiary,
        )
    }

    /// Bookkeeping, not a gesture: it must never cancel a passive pull or
    /// wait behind one.
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
