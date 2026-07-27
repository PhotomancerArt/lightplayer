//! Pre-flight summary for a node removal, computed client-side from the
//! synced inventory (no wire round-trip) for the delete confirmation.

use crate::ActionConfirmation;

/// What removing one node would do, as far as the client can tell from its
/// mirror: dependents that reference the node, pending edits the removal
/// would sweep, and the files expected to be staged for deletion.
///
/// Best-effort by design — the server's `RemoveNode` validation is the
/// authority (shared assets are never deleted there; unknown sites reject).
/// The web layer composes the confirmation dialog from this; the node
/// header's delete action carries [`Self::confirmation`] pre-composed.
#[derive(Clone, Debug, PartialEq)]
pub struct UiNodeRemovePreflight {
    /// Display label of the node being removed.
    pub node_label: String,
    /// Other nodes that reference this node or its subtree: authored
    /// bindings from outside the subtree plus surviving uses of a subtree
    /// def artifact (`node:` refs / playlist entries elsewhere).
    pub dependent_count: usize,
    /// Pending edits under the subtree that the removal sweeps (they are
    /// NOT restored by reverting the removal).
    pub pending_edit_count: usize,
    /// Project files expected to be staged for deletion (def files of the
    /// subtree plus client-resolvable exclusive assets).
    pub staged_files: Vec<String>,
}

impl UiNodeRemovePreflight {
    /// Compose the delete confirmation for this removal (same
    /// `ActionConfirmation` pattern as `HomeOp::DeletePackage`).
    pub fn confirmation(&self) -> ActionConfirmation {
        let mut message = format!("Remove {} from the project?", self.node_label);
        if !self.staged_files.is_empty() {
            message.push_str(&format!(
                " {} file(s) will be deleted on save: {}.",
                self.staged_files.len(),
                self.staged_files.join(", ")
            ));
        }
        if self.pending_edit_count > 0 {
            message.push_str(&format!(
                " {} pending edit(s) on it will be discarded.",
                self.pending_edit_count
            ));
        }
        if self.dependent_count > 0 {
            message.push_str(&format!(
                " {} other node(s) reference it and may error.",
                self.dependent_count
            ));
        }
        message.push_str(" You can revert from the save panel until you save.");
        ActionConfirmation::new("Delete node", message, "Delete")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_composes_files_edits_and_dependents() {
        let preflight = UiNodeRemovePreflight {
            node_label: "Orbit shader".to_string(),
            dependent_count: 2,
            pending_edit_count: 1,
            staged_files: vec!["/orbit.json".to_string(), "/orbit.glsl".to_string()],
        };

        let confirmation = preflight.confirmation();
        assert_eq!(confirmation.title, "Delete node");
        assert_eq!(confirmation.confirm_label, "Delete");
        assert!(confirmation.message.contains("Remove Orbit shader"));
        assert!(
            confirmation
                .message
                .contains("2 file(s) will be deleted on save: /orbit.json, /orbit.glsl")
        );
        assert!(confirmation.message.contains("1 pending edit(s)"));
        assert!(confirmation.message.contains("2 other node(s)"));
    }

    #[test]
    fn clean_leaf_confirmation_stays_minimal() {
        let preflight = UiNodeRemovePreflight {
            node_label: "Clock".to_string(),
            dependent_count: 0,
            pending_edit_count: 0,
            staged_files: Vec::new(),
        };

        let message = preflight.confirmation().message;
        assert!(message.contains("Remove Clock"));
        assert!(!message.contains("pending edit"));
        assert!(!message.contains("reference"));
        assert!(message.contains("revert from the save panel"));
    }
}
