//! Device-push ops: everything that moves a project onto hardware.
//!
//! The deploy DIALOG is gone (M8′ — provisioning runs through the
//! card's states and sheets; see the 2026-07-24 replan). What survives
//! is the card-native verb set: the direct push (M5's lane) and the
//! D30 diverged verbs. Routed like home ops — no controller struct of
//! its own; the `StudioController` executes the effects against the
//! live device session.

use core::any::Any;

use crate::{ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_LOAD_DEADLINE};

/// The node id device-push actions target.
pub const DEPLOY_NODE_ID: &str = "studio|deploy";

/// A push's resolved library target (was the dialog era's review
/// subject; survives as the direct push's resolution — key → concrete
/// head).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeployTarget {
    /// `prj_…` uid.
    pub project_uid: String,
    /// Library slug at resolve time (display).
    pub slug: String,
    /// The head content hash the push replaces the device's copy with.
    pub head: lpc_history::ContentHash,
    /// The head's version number on the project line, for the
    /// "Pushing vN" narration.
    pub version_number: Option<usize>,
}

/// One device-push gesture.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeployOp {
    /// Push a library head to the connected device DIRECTLY (M5): the
    /// dispatching gesture (the card's Push button, the Project-tab
    /// picker row, the drop-confirm sheet's verb, the project card's
    /// Push-to row) IS the D11 consent. Progress folds into the card's
    /// Operation-in-flight state.
    PushProject {
        /// The library project to push.
        key: String,
        /// The board to push it TO (M4) — the card the gesture came from.
        target: crate::DeviceTarget,
    },
    /// Diverged verb (D11/D30): the device's copy becomes the project's
    /// new head (it was banked at connect). Dispatched from the card's
    /// drift sheet; resolves the copy from that card's session sync.
    AdoptDeviceCopy { target: crate::DeviceTarget },
    /// Diverged verb (D11/D30): fork the device's copy into a new
    /// project named after the device; the line stays where it is.
    KeepBothFork { target: crate::DeviceTarget },
    /// Format verb (P5): the board holds a project at an older format
    /// this build can migrate. Migrate it in the LIBRARY and push the
    /// result — the device is never rewritten in place (D14 / ADR
    /// 2026-07-05 decision 5).
    ///
    /// A deploy op because it ends in a push: same target resolution,
    /// same hash-checked `open_library_project` lane, same
    /// Operation-in-flight narration. Non-destructive (the pre-migration
    /// copy stays in project history), so it dispatches from the card
    /// face without a confirm — like `UseBoardCopy`.
    UpgradeDeviceProject { target: crate::DeviceTarget },
    /// Erase the device's flash (firmware op; destructive). The card's
    /// Danger tab carries it behind the D41 confirm sheet.
    EraseDevice { target: crate::DeviceTarget },
}

impl ControllerOp for DeployOp {
    fn default_action_meta(&self) -> ActionMeta {
        match self {
            Self::PushProject { .. } => ActionMeta::new(
                "Push",
                "Push your newest version to this device. Its current \
                 contents are already saved in your library.",
                ActionPriority::Primary,
            )
            .with_icon("upload"),
            Self::AdoptDeviceCopy { .. } => ActionMeta::new(
                "Adopt device version",
                "Make the device's copy this project's newest version.",
                ActionPriority::Secondary,
            ),
            Self::KeepBothFork { .. } => ActionMeta::new(
                "Keep both",
                "Save the device's copy as its own project, named after \
                 the device.",
                ActionPriority::Secondary,
            )
            .with_icon("copy"),
            Self::UpgradeDeviceProject { .. } => ActionMeta::new(
                "Upgrade project",
                "Bring this board's project up to the format this Studio \
                 uses, and put it back on the board. The copy that is there \
                 now is already saved in your library.",
                ActionPriority::Primary,
            )
            .with_icon("upload"),
            Self::EraseDevice { .. } => ActionMeta::new(
                "Erase device…",
                "Erase the device's flash storage entirely.",
                ActionPriority::Tertiary,
            )
            .with_icon("remove")
            .destructive(),
        }
    }

    fn action_class(&self) -> ActionClass {
        // everything here talks to the device and gets the long budget
        // (push and erase move real bytes over serial)
        ActionClass::Foreground {
            deadline: PROJECT_LOAD_DEADLINE,
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
