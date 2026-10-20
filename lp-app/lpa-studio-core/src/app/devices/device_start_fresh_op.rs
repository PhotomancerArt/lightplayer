//! "Start fresh": the way out for a board whose files are gone (the
//! connected feel gate, Yona: "what am I supposed to do there? what if I
//! just want to reset it?").
//!
//! Not a model action, and nothing is written to the board: its files are
//! already gone. Studio stops saying so, and the board waits, empty, for a
//! new project (Add a project, its primary). The choice is remembered per
//! board in this browser ([`super::BoardFilesMemory`]) until the board runs
//! a project again, so a reload does not bring the notice back.

use core::any::Any;

use lpa_devices::DeviceId;

use crate::{ActionClass, ActionMeta, ActionPriority, ControllerOp};

/// Let a board's lost files go.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceStartFreshOp {
    pub device: DeviceId,
}

impl DeviceStartFreshOp {
    /// Routed by `StudioController` directly, like the roster's own ops.
    pub const NODE_ID: &'static str = "studio|device-start-fresh";

    /// This choice as a dispatchable [`UiAction`](crate::UiAction).
    pub fn action_for(device: DeviceId) -> crate::UiAction {
        crate::UiAction::from_op(crate::ControllerId::new(Self::NODE_ID), Self { device })
    }
}

impl ControllerOp for DeviceStartFreshOp {
    /// Routine: it loses nothing that is not already gone. A backup this
    /// browser kept stays where it is, and Download backup still offers it.
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Start fresh",
            "Let this board's old files go and leave it empty, ready for a new project. \
             Nothing on the board changes, and any backup stays where it is.",
            ActionPriority::Secondary,
        )
        .with_icon("add")
    }

    /// Nothing crosses the wire: it must never cancel a pull.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_fresh_is_routine_and_the_agent_may_press_it() {
        let action = DeviceStartFreshOp::action_for(DeviceId(3));
        assert_eq!(action.meta().label, "Start fresh");
        assert!(action.meta().consequence.is_routine());
        assert!(!action.meta().needs_user());
        assert_eq!(
            action.op_as::<DeviceStartFreshOp>(),
            Some(&DeviceStartFreshOp {
                device: DeviceId(3)
            })
        );
    }
}
