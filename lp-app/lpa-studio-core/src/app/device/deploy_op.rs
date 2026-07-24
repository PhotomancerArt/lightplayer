//! Deploy-dialog ops: everything that moves a project onto hardware.
//!
//! Routed like home ops — no controller struct of its own; the
//! `StudioController` owns the [`DeploySession`](super::deploy_session)
//! and executes the effects.

use core::any::Any;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_ACTION_DEADLINE,
    PROJECT_LOAD_DEADLINE,
};

/// The node id deploy-dialog actions target.
pub const DEPLOY_NODE_ID: &str = "studio|deploy";

/// One deploy-dialog gesture.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeployOp {
    /// Open the dialog, optionally preselecting a push target (slug or
    /// `prj_…` uid — the editor's Push action and drag-drop pass one).
    /// With no explicit target, a device running a library-KNOWN project
    /// pre-targets that project — the dialog opens on Reviewing, with the
    /// picker one click away (defect 2026-07-23).
    OpenDialog {
        target_key: Option<String>,
    },
    CloseDialog,
    /// Flash the bundled firmware (wizard step 1, or a standalone
    /// firmware op — visually separate from deploy).
    FlashFirmware,
    /// Mint a `dev_` uid, write `/.lp/device.json`, register the device.
    StampIdentity {
        name: String,
    },
    /// Pick what to push (wizard step 3 / review's picker).
    ChoosePackage {
        key: String,
    },
    /// The one mutating confirmation: replace the device's project with
    /// the reviewed head, verify, record the push.
    ConfirmPush,
    /// Push a library head to the connected device DIRECTLY — no dialog
    /// (M5): the Running-behind card's Push button IS the D11 consent
    /// click. Progress folds into the card's Operation-in-flight state
    /// (the session's in-flight operation narrates the roster card).
    PushProject {
        key: String,
    },
    /// Diverged verb (D11): the device's copy becomes the project's new
    /// head (it was banked at connect).
    AdoptDeviceCopy,
    /// Diverged verb (D11): fork the device's copy into a new project
    /// named after the device; the line stays where it is.
    KeepBothFork,
    /// Erase the device's flash (firmware op; destructive).
    EraseDevice,
    /// Return to the step a failure interrupted.
    RetryFailed,
}

impl ControllerOp for DeployOp {
    fn default_action_meta(&self) -> ActionMeta {
        match self {
            Self::OpenDialog { .. } => ActionMeta::new(
                "Connect device…",
                "Open the device dialog: connect, provision, and push.",
                ActionPriority::Primary,
            )
            .with_icon("usb"),
            Self::CloseDialog => ActionMeta::new(
                "Close",
                "Close the device dialog.",
                ActionPriority::Tertiary,
            ),
            Self::FlashFirmware => ActionMeta::new(
                "Install firmware",
                "Flash LightPlayer firmware onto the connected ESP32.",
                ActionPriority::Primary,
            )
            .with_icon("zap"),
            Self::StampIdentity { .. } => ActionMeta::new(
                "Name this device",
                "Give the device a name and a permanent identity.",
                ActionPriority::Primary,
            )
            .with_icon("edit"),
            Self::ChoosePackage { .. } => ActionMeta::new(
                "Choose",
                "Push this project to the device.",
                ActionPriority::Primary,
            ),
            Self::ConfirmPush => ActionMeta::new(
                "Push",
                "Replace the device's project with this version. Its \
                 current contents are already saved in your library.",
                ActionPriority::Primary,
            )
            .with_icon("upload"),
            Self::PushProject { .. } => ActionMeta::new(
                "Push",
                "Push your newest version to this device. Its current \
                 contents are already saved in your library.",
                ActionPriority::Primary,
            )
            .with_icon("upload"),
            Self::AdoptDeviceCopy => ActionMeta::new(
                "Adopt device version",
                "Make the device's copy this project's newest version.",
                ActionPriority::Secondary,
            ),
            Self::KeepBothFork => ActionMeta::new(
                "Keep both",
                "Save the device's copy as its own project, named after \
                 the device.",
                ActionPriority::Secondary,
            )
            .with_icon("copy"),
            Self::EraseDevice => ActionMeta::new(
                "Erase device…",
                "Erase the device's flash storage entirely.",
                ActionPriority::Tertiary,
            )
            .with_icon("remove")
            .destructive(),
            Self::RetryFailed => ActionMeta::new(
                "Retry",
                "Try the failed step again.",
                ActionPriority::Primary,
            ),
        }
    }

    fn action_class(&self) -> ActionClass {
        match self {
            // dialog open/close and picker moves are local state
            Self::OpenDialog { .. } | Self::CloseDialog | Self::RetryFailed => {
                ActionClass::Foreground {
                    deadline: PROJECT_ACTION_DEADLINE,
                }
            }
            // everything that talks to the device gets the long budget
            // (flash and push move real bytes over serial)
            Self::FlashFirmware
            | Self::StampIdentity { .. }
            | Self::ChoosePackage { .. }
            | Self::ConfirmPush
            | Self::PushProject { .. }
            | Self::AdoptDeviceCopy
            | Self::KeepBothFork
            | Self::EraseDevice => ActionClass::Foreground {
                deadline: PROJECT_LOAD_DEADLINE,
            },
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
