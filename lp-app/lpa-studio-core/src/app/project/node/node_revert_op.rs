//! Node-level batch revert operation.

use core::any::Any;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
    ProjectNodeAddress,
};

/// Revert every pending edit under one node's subtree (M3 UX gate feedback):
/// the node's own edit entries plus its descendant nodes', matching the
/// subtree [`crate::DirtySummary`] the node header announces.
///
/// Dispatched to `ProjectController::NODE_ID` like [`crate::SlotEditOp`]; the
/// controller enumerates the entries through the edit join and expands the op
/// into per-entry `RemoveSlotEdit` wire mutations sent as **one** batch — one
/// wire round-trip, one mirror snapshot. Like `Revert`, it never coalesces in
/// the studio actor queue (only `SetValue` coalesces) and acts as a
/// coalescing barrier.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeRevertOp {
    /// Address of the node whose subtree edits are discarded.
    pub node: ProjectNodeAddress,
}

impl ControllerOp for NodeRevertOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Revert node edits",
            "Discard every pending edit under this node.",
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
    fn node_revert_is_editor_foreground_class_with_revert_meta() {
        let op = NodeRevertOp {
            node: ProjectNodeAddress::parse("/demo.project/pixels.fixture").unwrap(),
        };

        assert_eq!(
            op.action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_EDITOR_ACTION_DEADLINE,
            }
        );
        assert_eq!(op.default_action_meta().label, "Revert node edits");
        assert_eq!(op.default_action_meta().priority, ActionPriority::Secondary);
    }
}
