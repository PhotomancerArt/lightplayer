//! "Download backup": hand the user a board's backup as a ZIP (the C6
//! repartition, plan MQ4 — the backup is always offered as a download, and
//! required when the browser could not store it).
//!
//! Not a model action: the model never holds a board's files. The effects
//! layer fetches the bytes (the inspection's staged archive, or the stored
//! backup a restore would put back) and the view carries them out to the
//! web shell, which downloads when it sees a new `seq`.

use core::any::Any;

use lpa_devices::DeviceId;

use crate::{ActionClass, ActionMeta, ActionPriority, ControllerOp};

/// Download a board's backup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceBackupOp {
    pub device: DeviceId,
}

impl DeviceBackupOp {
    /// Routed by `StudioController` directly, like the roster's own ops.
    pub const NODE_ID: &'static str = "studio|device-backup";

    /// This download as a dispatchable [`UiAction`](crate::UiAction).
    pub fn action_for(device: DeviceId) -> crate::UiAction {
        crate::UiAction::from_op(crate::ControllerId::new(Self::NODE_ID), Self { device })
    }
}

impl ControllerOp for DeviceBackupOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Download backup",
            "Save a copy of this board's files to your computer as a ZIP.",
            ActionPriority::Secondary,
        )
        .with_icon("download")
        // A browser download wants a real click.
        .needs_user_activation()
    }

    /// Reading a backup changes nothing: it must never cancel a pull.
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
