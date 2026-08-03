//! Node creation operation and its UI attachment vocabulary.

use core::any::Any;

use lpc_model::NodeKind;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
    ProjectNodeAddress,
};

/// Where a UI create gesture attaches its new node. The controller resolves
/// this to the wire [`lpc_model::NodeAttachSite`]: the project root maps to
/// `ProjectNodes { key }` with the auto-generated node name as the key; a
/// playlist maps to `Slot { artifact, path: entries[k].node }` with `k`
/// picked by the entries map's suggested-key rule (first free index).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiAttachTarget {
    /// The project root's `nodes` map.
    ProjectRoot,
    /// A playlist node's `entries` map, addressed by the playlist's stable
    /// node address.
    Playlist { node: ProjectNodeAddress },
}

/// Create one blank node of `kind` at `attach` (dedicated `CreateNode` wire
/// op, commit-immediate server-side).
///
/// Dispatched to `ProjectController::NODE_ID` like [`crate::NodeRevertOp`];
/// the controller auto-names the node (kind slug + `_2`/`_3` dedup), builds
/// the starter bytes ([`lpc_model::starter_for_kind`], bare
/// `default_for_kind` fallback), sends one `CreateNode`, and on ack runs an
/// immediate project refresh (tree deltas ride `ProjectRead`, not the ack),
/// the save-pull (creation lands as a `Saved` library event), and focuses
/// the new node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeCreateOp {
    /// The node kind to create (every kind except `Project` is offered by
    /// the picker — see [`crate::UiAddNodeMenu`]).
    pub kind: NodeKind,
    /// Where the new node attaches.
    pub attach: UiAttachTarget,
}

impl ControllerOp for NodeCreateOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Add node",
            "Create a new node in this project.",
            ActionPriority::Secondary,
        )
        .with_icon("add")
    }

    fn action_class(&self) -> ActionClass {
        // Same editor foreground class as the other project edit ops.
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
    fn node_create_is_editor_foreground_class_with_add_meta() {
        let op = NodeCreateOp {
            kind: NodeKind::Shader,
            attach: UiAttachTarget::ProjectRoot,
        };

        assert_eq!(
            op.action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_EDITOR_ACTION_DEADLINE,
            }
        );
        assert_eq!(op.default_action_meta().label, "Add node");
        assert_eq!(op.default_action_meta().icon.as_deref(), Some("add"));
    }

    #[test]
    fn attach_targets_compare_by_site() {
        let playlist = UiAttachTarget::Playlist {
            node: ProjectNodeAddress::parse("/demo.module/loop.playlist").unwrap(),
        };
        assert_ne!(UiAttachTarget::ProjectRoot, playlist);
        assert_eq!(playlist.clone(), playlist);
    }
}
