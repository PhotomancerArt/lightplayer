//! Node-level Clear operation for Debug overrides.

use core::any::Any;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
    ProjectNodeAddress,
};

/// **Clear** every Debug override under one node's subtree (D7, the per-node
/// scope of the Clear verb): the node's own Debug edit entries plus its
/// descendant nodes'. Persisted edits are untouched — they are the Save
/// panel's business, and their verb stays Revert.
///
/// Dispatched to `ProjectController::NODE_ID` like [`crate::NodeRevertOp`];
/// the controller enumerates the Debug entries through the edit join and
/// expands the op into per-entry `RemoveSlotEdit` wire mutations sent as
/// **one** batch. A Debug slot has no durable authored value underneath, so
/// removing its overlay entry returns it to the shape default. Like
/// `NodeRevertOp` it never coalesces in the studio actor queue and acts as a
/// coalescing barrier.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeClearDebugOp {
    /// Address of the node whose subtree Debug overrides are cleared.
    pub node: ProjectNodeAddress,
}

impl ControllerOp for NodeClearDebugOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Clear debug",
            "Clear every debug override under this node.",
            ActionPriority::Secondary,
        )
    }

    fn action_class(&self) -> ActionClass {
        // Same editor foreground class as the slot-level edit ops.
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
    fn node_clear_debug_is_editor_foreground_class_with_clear_meta() {
        let op = NodeClearDebugOp {
            node: ProjectNodeAddress::parse("/demo.module/clock.clock").unwrap(),
        };

        assert_eq!(
            op.action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_EDITOR_ACTION_DEADLINE,
            }
        );
        // D7 vocabulary: Clear, never Revert/Reset.
        let meta = op.default_action_meta();
        assert_eq!(meta.label, "Clear debug");
        assert!(!meta.summary.contains("Revert"));
        assert!(!meta.summary.contains("Reset"));
        assert_eq!(meta.priority, ActionPriority::Secondary);
    }
}
