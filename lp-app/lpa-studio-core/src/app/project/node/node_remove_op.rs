//! Node removal operation.

use core::any::Any;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
    ProjectNodeAddress,
};

/// Remove the node at `node` (dedicated `RemoveNode` wire op, staged in the
/// overlay server-side until the next save).
///
/// Dispatched to `ProjectController::NODE_ID` like [`crate::NodeRevertOp`];
/// the controller resolves the address to the wire
/// [`lpc_model::NodeAttachSite`] (project `nodes[key]` for root children,
/// the playlist's `entries[k].node` slot for playlist entries), sends one
/// `RemoveNode`, and on ack converges the overlay mirror and refreshes so
/// the node disappears immediately. The staged removal stays revertible from
/// the save panel until save; the header action carrying this op wears an
/// [`crate::ActionConfirmation`] composed from the pre-flight summary
/// ([`crate::UiNodeRemovePreflight`]).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeRemoveOp {
    /// Stable address of the node to remove.
    pub node: ProjectNodeAddress,
}

impl ControllerOp for NodeRemoveOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Delete node",
            "Remove this node from the project; its files are deleted on save.",
            ActionPriority::Tertiary,
        )
        .with_icon("remove")
        .destructive()
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
    fn node_remove_is_editor_foreground_class_with_destructive_meta() {
        let op = NodeRemoveOp {
            node: ProjectNodeAddress::parse("/demo.module/orbit.shader").unwrap(),
        };

        assert_eq!(
            op.action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_EDITOR_ACTION_DEADLINE,
            }
        );
        let meta = op.default_action_meta();
        assert_eq!(meta.label, "Delete node");
        assert_eq!(meta.icon.as_deref(), Some("remove"));
        assert!(meta.destructive);
    }
}
