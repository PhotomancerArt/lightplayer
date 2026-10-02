//! Pre-flight summary for a node removal, computed client-side from the
//! synced inventory (no wire round-trip) for the remove action's summary.

/// What removing one node would do, as far as the client can tell from its
/// mirror: dependents that reference the node, pending edits the removal
/// would sweep, and the files expected to be staged for deletion.
///
/// Best-effort by design — the server's `RemoveNode` validation is the
/// authority (shared assets are never deleted there; unknown sites reject).
/// The node header's remove action carries [`Self::summary`] pre-composed as
/// its summary (its tooltip), and [`Self::consequence`] as its level: a
/// removal is Undoable (Revert brings the node back until save) unless it
/// sweeps pending edits, which no revert restores — then it is Lasting and
/// arms.
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
    /// Compose what this removal does, as the remove action's summary.
    pub fn summary(&self) -> String {
        let mut message = format!("Remove {} from the project.", self.node_label);
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
        message
    }

    /// How serious the removal is (D7). Undoable while reverting the
    /// removal gives everything back; Lasting when it also discards pending
    /// edits on the subtree, because those are gone for good.
    pub fn consequence(&self) -> crate::ActionConsequence {
        if self.pending_edit_count == 0 {
            return crate::ActionConsequence::Undoable;
        }
        crate::ActionConsequence::Lasting(crate::ActionConfirmation::new(
            format!("Remove {}?", self.node_label),
            format!(
                "Its {} unsaved edit(s) are discarded, and reverting the removal does not bring them back.",
                self.pending_edit_count
            ),
            "remove",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_composes_files_edits_and_dependents() {
        let preflight = UiNodeRemovePreflight {
            node_label: "Orbit shader".to_string(),
            dependent_count: 2,
            pending_edit_count: 1,
            staged_files: vec!["/orbit.json".to_string(), "/orbit.glsl".to_string()],
        };

        let summary = preflight.summary();
        assert!(summary.contains("Remove Orbit shader"));
        assert!(summary.contains("2 file(s) will be deleted on save: /orbit.json, /orbit.glsl"));
        assert!(summary.contains("1 pending edit(s)"));
        assert!(summary.contains("2 other node(s)"));
    }

    #[test]
    fn clean_leaf_summary_stays_minimal() {
        let preflight = UiNodeRemovePreflight {
            node_label: "Clock".to_string(),
            dependent_count: 0,
            pending_edit_count: 0,
            staged_files: Vec::new(),
        };

        let message = preflight.summary();
        assert!(message.contains("Remove Clock"));
        assert!(!message.contains("pending edit"));
        assert!(!message.contains("reference"));
        assert!(message.contains("revert from the save panel"));
        assert_eq!(preflight.consequence(), crate::ActionConsequence::Undoable);
    }

    #[test]
    fn a_removal_that_sweeps_unsaved_edits_is_lasting() {
        let preflight = UiNodeRemovePreflight {
            node_label: "Orbit shader".to_string(),
            dependent_count: 0,
            pending_edit_count: 2,
            staged_files: Vec::new(),
        };

        assert!(preflight.consequence().arms());
        let copy = preflight.consequence();
        assert!(copy.copy().unwrap().message.contains("2 unsaved edit(s)"));
    }
}
