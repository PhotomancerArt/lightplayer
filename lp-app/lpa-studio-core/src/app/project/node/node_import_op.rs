//! The **import** op: vendor one library pattern export into the open
//! project (module authoring unit, P5).
//!
//! Planning Q3 ruling: the gesture starts INSIDE the open project — the
//! destination is unambiguous, because the project is open by definition.
//! So this is an add-node source, not a card verb, and it rides the same
//! `CreateNode` wire command every other create does: the bytes just come
//! from another package in the library instead of a starter template.
//!
//! Copy-to-own (D-vendoring): what lands is the user's own copy of the
//! folder — no link back, no read-only bits, no hidden directory. The only
//! trace of where it came from is the module's own `provenance`, stamped
//! from the source project's manifest when the export carried none (R14).

use core::any::Any;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
};

use super::node_create_op::UiAttachTarget;

/// Vendor `export` from library package `package_uid` into this project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeImportOp {
    /// Source package `prj_…` uid, straight off the picker row.
    pub package_uid: String,
    /// The export folder's name in that package (`effect`, `fire`).
    pub export: String,
    /// Where the vendored module attaches. Project root this round — the
    /// field is here because the picker's other sources carry it and the
    /// attach vocabulary is one thing.
    pub attach: UiAttachTarget,
}

impl ControllerOp for NodeImportOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Import pattern",
            "Copy a pattern module from your library into this project.",
            ActionPriority::Secondary,
        )
        .with_icon("add")
    }

    fn action_class(&self) -> ActionClass {
        // Reads one library package and sends one `CreateNode` — the same
        // editor foreground budget the other create sources take.
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
    fn import_is_editor_foreground_class() {
        let op = NodeImportOp {
            package_uid: "prj_a".to_string(),
            export: "effect".to_string(),
            attach: UiAttachTarget::ProjectRoot,
        };
        assert_eq!(
            op.action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_EDITOR_ACTION_DEADLINE,
            }
        );
        assert_eq!(op.default_action_meta().label, "Import pattern");
    }

    /// Two exports of the same package are different ops — a family's rows
    /// must never dedupe into one another.
    #[test]
    fn each_export_is_its_own_op() {
        let fire = NodeImportOp {
            package_uid: "prj_a".to_string(),
            export: "fire".to_string(),
            attach: UiAttachTarget::ProjectRoot,
        };
        let ice = NodeImportOp {
            export: "ice".to_string(),
            ..fire.clone()
        };
        assert!(fire.eq_op(&fire.clone()));
        assert!(!fire.eq_op(&ice));
    }
}
