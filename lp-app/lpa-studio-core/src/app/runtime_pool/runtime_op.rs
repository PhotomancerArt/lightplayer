//! Verbs that act on the attached RUNTIME rather than on a project.
//!
//! Two survived the device-system teardown (M2 of the device-model
//! rebuild), where they lived as `DeviceOp` variants alongside a dozen
//! hardware verbs: stopping the simulator (the sim card's danger zone) and
//! setting the runtime's log level (the console's runtime-level selector).
//! Both are runtime-scoped and neither is device-specific, so they get
//! their own small op rather than riding a project op or waiting for the
//! rebuilt device model.

use core::any::Any;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_ACTION_DEADLINE, UiLogLevel,
};

/// One runtime-scoped gesture.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeOp {
    /// Destroy the simulator session (worker + wire client). The card's
    /// danger-zone verb; unsaved changes on it are gone (D22 — the sim
    /// persists nothing).
    StopSimulator,
    /// Ask the attached runtime's server to apply a log level. There is no
    /// read-back on the wire, so the console's selector shows the last
    /// requested level optimistically.
    SetLogLevel { level: UiLogLevel },
    /// Open the editor as a lens on a roster device (round-2 M5): borrow the
    /// device's wire, install a device session, attach the lens. `uid` is
    /// the device's registered `dev…` uid — the `/device/<uid>` address.
    OpenDeviceLens { uid: String },
    /// Close the device lens: detach the editor, drop the device session,
    /// give the wire back to the roster. The device itself stays on its
    /// card — nothing about the board changes.
    CloseDeviceLens,
}

impl RuntimeOp {
    /// The node id runtime actions target. Routed by `StudioController`
    /// directly — there is no controller struct behind it.
    pub const NODE_ID: &'static str = "studio|runtime";
}

impl ControllerOp for RuntimeOp {
    fn default_action_meta(&self) -> ActionMeta {
        match self {
            Self::StopSimulator => ActionMeta::new(
                "Stop simulator",
                "Shut the simulator down. Anything running on it stops, and \
                 unsaved changes on it are lost.",
                ActionPriority::Tertiary,
            )
            .with_icon("remove")
            .destructive(),
            Self::SetLogLevel { .. } => ActionMeta::new(
                "Set log level",
                "Ask the runtime's server to log at this level.",
                ActionPriority::Tertiary,
            ),
            Self::OpenDeviceLens { .. } => ActionMeta::new(
                "Open",
                "Open this board in the editor.",
                ActionPriority::Primary,
            )
            .with_icon("open"),
            Self::CloseDeviceLens => ActionMeta::new(
                "Close",
                "Close the editor on this board and give its wire back.",
                ActionPriority::Tertiary,
            ),
        }
    }

    fn action_class(&self) -> ActionClass {
        match self {
            // Stopping the sim tears the session down; it owns the
            // connection for the duration, so it carries no deadline.
            Self::StopSimulator => ActionClass::Recovery,
            // One small wire write.
            Self::SetLogLevel { .. } => ActionClass::Foreground {
                deadline: PROJECT_ACTION_DEADLINE,
            },
            // The attach is a handful of wire reads (loaded projects, the
            // project's inventory) on a board that just said hello.
            Self::OpenDeviceLens { .. } => ActionClass::Foreground {
                deadline: PROJECT_ACTION_DEADLINE,
            },
            // Tears the session down; it owns the connection for the
            // duration, so it carries no deadline (like stopping the sim).
            Self::CloseDeviceLens => ActionClass::Recovery,
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
    fn stopping_the_sim_owns_the_connection_and_reads_destructive() {
        assert_eq!(
            RuntimeOp::StopSimulator.action_class(),
            ActionClass::Recovery
        );
        assert!(RuntimeOp::StopSimulator.default_action_meta().destructive);
    }

    #[test]
    fn opening_a_device_lens_is_bounded_and_closing_it_owns_the_connection() {
        assert_eq!(
            RuntimeOp::OpenDeviceLens {
                uid: "devabc".to_string()
            }
            .action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_ACTION_DEADLINE,
            }
        );
        assert_eq!(
            RuntimeOp::CloseDeviceLens.action_class(),
            ActionClass::Recovery
        );
        assert!(!RuntimeOp::CloseDeviceLens.default_action_meta().destructive);
    }

    #[test]
    fn setting_the_log_level_is_one_bounded_wire_write() {
        assert_eq!(
            RuntimeOp::SetLogLevel {
                level: UiLogLevel::Debug
            }
            .action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_ACTION_DEADLINE,
            }
        );
    }
}
