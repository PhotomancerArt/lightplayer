//! The export **designation** op: add or remove one module folder from the
//! open project's `exports` list (module authoring unit, P3).
//!
//! The gesture lives on the module's own detail popup — you designate the
//! thing you are looking at (spike §2) — but what it edits is the project
//! CONTAINER manifest, which is library-owned. So the op names a folder,
//! not a node: the manifest's vocabulary is folder names, and the popup
//! already resolved which folder this card is.
//!
//! Why not a [`crate::app::library::CatalogOp`], the way `Rename` patches
//! `project.json`? Because this project is OPEN in this tab. A catalog
//! transaction takes the target project's lock first and refuses
//! `OpenInThisTab`; the open project's own exclusive-locked `package_fs` is
//! already in hand, and it is the same handle `active_manifest` and the
//! export lint read. The controller writes through it and mirrors the
//! bytes into the runtime copy so the save-path hash tripwire stays quiet.

use core::any::Any;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
};

/// Designate (or un-designate) one module folder as a project export.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleExportOp {
    /// The module folder name, relative to the project root (`fire` for
    /// `/fire/module.json`).
    pub folder: String,
    /// `true` adds the folder to `exports`, `false` removes it.
    pub export: bool,
}

impl ControllerOp for ModuleExportOp {
    fn default_action_meta(&self) -> ActionMeta {
        if self.export {
            ActionMeta::new(
                "Export module",
                "Ship this module's folder from this project.",
                ActionPriority::Secondary,
            )
        } else {
            ActionMeta::new(
                "Stop exporting module",
                "Remove this module's folder from the project's exports.",
                ActionPriority::Secondary,
            )
        }
    }

    fn action_class(&self) -> ActionClass {
        ActionClass::Foreground {
            deadline: PROJECT_EDITOR_ACTION_DEADLINE,
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
    fn designation_carries_the_editor_foreground_class() {
        let op = ModuleExportOp {
            folder: "fire".to_string(),
            export: true,
        };
        assert_eq!(
            op.action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_EDITOR_ACTION_DEADLINE,
            }
        );
    }

    /// The direction is part of the op's identity: "start exporting fire"
    /// and "stop exporting fire" must never dedupe into one another.
    #[test]
    fn the_two_directions_are_different_ops() {
        let on = ModuleExportOp {
            folder: "fire".to_string(),
            export: true,
        };
        let off = ModuleExportOp {
            folder: "fire".to_string(),
            export: false,
        };
        assert!(on.eq_op(&on.clone()));
        assert!(!on.eq_op(&off));
        assert_ne!(
            on.default_action_meta().label,
            off.default_action_meta().label
        );
    }
}
