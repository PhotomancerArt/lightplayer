//! Apply tree deltas to the client-side mirror.
//!
//! See `docs/roadmaps/2026-04-28-node-runtime/design/07-sync.md`.

use alloc::vec::Vec;
use lpc_model::{FrameId, NodeId};
use lpc_wire::WireTreeDelta;

use super::{NodeTreeView, TreeEntryView};

/// Error from applying a delta.
#[derive(Clone, Debug, PartialEq)]
pub enum ApplyError {
    /// Tried to update an entry that doesn't exist.
    MissingNode(NodeId),
    /// Entry exists but with a different parent (path mismatch).
    ParentMismatch {
        id: NodeId,
        expected: Option<NodeId>,
        actual: Option<NodeId>,
    },
}

impl core::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ApplyError::MissingNode(id) => write!(f, "cannot apply delta: missing node {id}"),
            ApplyError::ParentMismatch {
                id,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "parent mismatch for {id}: expected {expected:?}, actual {actual:?}"
                )
            }
        }
    }
}

impl core::error::Error for ApplyError {}

/// Apply a single delta to the client tree.
///
/// - `Created`: Insert the new entry. If parent doesn't exist yet, the entry
///   will be orphaned until the parent's `Created` arrives (servers should emit
///   in parent-before-child order).
/// - `EntryChanged`: Update status, state, and change_frame.
/// - `ChildrenChanged`: Replace children list, update children_ver, and
///   **infer removals** by recursively removing any previous children that
///   are no longer in the new list.
pub fn apply_tree_delta(
    tree: &mut NodeTreeView,
    delta: &WireTreeDelta,
    frame: FrameId,
) -> Result<(), ApplyError> {
    match delta {
        WireTreeDelta::Created {
            id,
            path,
            parent,
            child_kind,
            children,
            status,
            state,
            created_frame,
            change_frame,
            children_ver,
        } => {
            let mut entry = TreeEntryView::new(
                *id,
                path.clone(),
                *parent,
                child_kind.clone(),
                status.clone(),
                state.clone(),
                *created_frame,
                *change_frame,
                *children_ver,
            );
            entry.children = children.clone();
            tree.insert(entry);
        }

        WireTreeDelta::EntryChanged {
            id,
            status,
            state,
            change_frame,
        } => {
            let entry = tree.get_mut(*id).ok_or(ApplyError::MissingNode(*id))?;
            entry.status = status.clone();
            entry.state = state.clone();
            entry.change_frame = *change_frame;
        }

        WireTreeDelta::ChildrenChanged {
            id,
            children,
            children_ver,
        } => {
            // Get the old children list to compute the diff
            let old_children: Vec<NodeId> = tree
                .get(*id)
                .map(|e| e.children.clone())
                .unwrap_or_default();

            // Update the entry
            let entry = tree.get_mut(*id).ok_or(ApplyError::MissingNode(*id))?;
            entry.children = children.clone();
            entry.children_ver = *children_ver;

            // Infer removals: any old child not in new list is removed
            let new_children_set: alloc::collections::BTreeSet<NodeId> =
                children.iter().copied().collect();
            for old_child in old_children {
                if !new_children_set.contains(&old_child) {
                    // Recursively remove this child and its descendants
                    tree.remove_subtree(old_child);
                }
            }
        }
    }

    // Update the synced frame
    tree.update_synced_frame(frame);

    Ok(())
}

/// Apply multiple deltas in order.
pub fn apply_tree_deltas(
    tree: &mut NodeTreeView,
    deltas: &[WireTreeDelta],
    frame: FrameId,
) -> Result<(), ApplyError> {
    for delta in deltas {
        apply_tree_delta(tree, delta, frame)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{NodeTreeView, TreeEntryView, apply_tree_delta};
    use lpc_model::{FrameId, NodeId, NodeName, TreePath};
    use lpc_wire::{WireChildKind, WireEntryState, WireNodeStatus, WireSlotIndex, WireTreeDelta};

    fn make_tree_with_root() -> NodeTreeView {
        let mut tree = NodeTreeView::new();
        let root = TreeEntryView::new(
            NodeId::new(0),
            TreePath::parse("/root.show").unwrap(),
            None,
            None,
            WireNodeStatus::Created,
            WireEntryState::Pending,
            FrameId::new(0),
            FrameId::new(0),
            FrameId::new(0),
        );
        tree.insert(root);
        tree
    }

    #[test]
    fn apply_created_adds_entry() {
        let mut tree = make_tree_with_root();

        let delta = WireTreeDelta::Created {
            id: NodeId::new(1),
            path: TreePath::parse("/root.show/child.vis").unwrap(),
            parent: Some(NodeId::new(0)),
            child_kind: Some(WireChildKind::Input {
                source: WireSlotIndex(0),
            }),
            children: alloc::vec![],
            status: WireNodeStatus::Created,
            state: WireEntryState::Pending,
            created_frame: FrameId::new(1),
            change_frame: FrameId::new(1),
            children_ver: FrameId::new(1),
        };

        apply_tree_delta(&mut tree, &delta, FrameId::new(1)).unwrap();

        assert_eq!(tree.len(), 2);
        assert!(tree.get(NodeId::new(1)).is_some());
    }

    #[test]
    fn apply_entry_changed_updates_status() {
        let mut tree = make_tree_with_root();

        // Add a child first
        let child = TreeEntryView::new(
            NodeId::new(1),
            TreePath::parse("/root.show/child.vis").unwrap(),
            Some(NodeId::new(0)),
            Some(WireChildKind::Input {
                source: WireSlotIndex(0),
            }),
            WireNodeStatus::Created,
            WireEntryState::Pending,
            FrameId::new(1),
            FrameId::new(1),
            FrameId::new(1),
        );
        tree.insert(child);

        // Apply status change
        let delta = WireTreeDelta::EntryChanged {
            id: NodeId::new(1),
            status: WireNodeStatus::Ok,
            state: WireEntryState::Alive,
            change_frame: FrameId::new(5),
        };

        apply_tree_delta(&mut tree, &delta, FrameId::new(5)).unwrap();

        let entry = tree.get(NodeId::new(1)).unwrap();
        assert!(matches!(entry.status, WireNodeStatus::Ok));
        assert!(matches!(entry.state, WireEntryState::Alive));
        assert_eq!(entry.change_frame.0, 5);
    }

    #[test]
    fn apply_children_changed_updates_list() {
        let mut tree = make_tree_with_root();

        // Add a child
        let child = TreeEntryView::new(
            NodeId::new(1),
            TreePath::parse("/root.show/child.vis").unwrap(),
            Some(NodeId::new(0)),
            Some(WireChildKind::Input {
                source: WireSlotIndex(0),
            }),
            WireNodeStatus::Created,
            WireEntryState::Pending,
            FrameId::new(1),
            FrameId::new(1),
            FrameId::new(1),
        );
        tree.insert(child);

        // Update root's children list
        let delta = WireTreeDelta::ChildrenChanged {
            id: NodeId::new(0),
            children: alloc::vec![NodeId::new(1)],
            children_ver: FrameId::new(2),
        };

        apply_tree_delta(&mut tree, &delta, FrameId::new(2)).unwrap();

        let root = tree.get(NodeId::new(0)).unwrap();
        assert_eq!(root.children, alloc::vec![NodeId::new(1)]);
        assert_eq!(root.children_ver.0, 2);
    }

    #[test]
    fn apply_children_changed_infers_removal() {
        let mut tree = make_tree_with_root();

        // Add two children
        let a = TreeEntryView::new(
            NodeId::new(1),
            TreePath::parse("/root.show/a.vis").unwrap(),
            Some(NodeId::new(0)),
            Some(WireChildKind::Input {
                source: WireSlotIndex(0),
            }),
            WireNodeStatus::Created,
            WireEntryState::Pending,
            FrameId::new(1),
            FrameId::new(1),
            FrameId::new(1),
        );
        let b = TreeEntryView::new(
            NodeId::new(2),
            TreePath::parse("/root.show/b.vis").unwrap(),
            Some(NodeId::new(0)),
            Some(WireChildKind::Input {
                source: WireSlotIndex(1),
            }),
            WireNodeStatus::Created,
            WireEntryState::Pending,
            FrameId::new(1),
            FrameId::new(1),
            FrameId::new(1),
        );
        tree.insert(a);
        tree.insert(b);

        // Set up root's children list
        {
            let root = tree.get_mut(NodeId::new(0)).unwrap();
            root.children = alloc::vec![NodeId::new(1), NodeId::new(2)];
        }

        // Apply delta that removes b from children
        let delta = WireTreeDelta::ChildrenChanged {
            id: NodeId::new(0),
            children: alloc::vec![NodeId::new(1)], // b is gone
            children_ver: FrameId::new(5),
        };

        apply_tree_delta(&mut tree, &delta, FrameId::new(5)).unwrap();

        // b should be removed
        assert!(tree.get(NodeId::new(1)).is_some());
        assert!(tree.get(NodeId::new(2)).is_none()); // b removed
        assert_eq!(tree.len(), 2); // root + a
    }

    #[test]
    fn apply_children_changed_recursively_removes_descendants() {
        let mut tree = make_tree_with_root();

        // Create grandchild -> parent -> root
        let parent = TreeEntryView::new(
            NodeId::new(1),
            TreePath::parse("/root.show/parent.vis").unwrap(),
            Some(NodeId::new(0)),
            Some(WireChildKind::Sidecar {
                name: NodeName::parse("parent").unwrap(),
            }),
            WireNodeStatus::Created,
            WireEntryState::Pending,
            FrameId::new(1),
            FrameId::new(1),
            FrameId::new(1),
        );
        let grandchild = TreeEntryView::new(
            NodeId::new(2),
            TreePath::parse("/root.show/parent.vis/grand.fx").unwrap(),
            Some(NodeId::new(1)),
            Some(WireChildKind::Input {
                source: WireSlotIndex(0),
            }),
            WireNodeStatus::Created,
            WireEntryState::Pending,
            FrameId::new(2),
            FrameId::new(2),
            FrameId::new(2),
        );
        tree.insert(parent);
        tree.insert(grandchild);

        // Set up children lists
        {
            let root = tree.get_mut(NodeId::new(0)).unwrap();
            root.children = alloc::vec![NodeId::new(1)];
        }
        {
            let parent = tree.get_mut(NodeId::new(1)).unwrap();
            parent.children = alloc::vec![NodeId::new(2)];
        }

        // Remove parent from root's children
        let delta = WireTreeDelta::ChildrenChanged {
            id: NodeId::new(0),
            children: alloc::vec![], // parent is gone
            children_ver: FrameId::new(5),
        };

        apply_tree_delta(&mut tree, &delta, FrameId::new(5)).unwrap();

        // Both parent and grandchild should be removed
        assert!(tree.get(NodeId::new(1)).is_none());
        assert!(tree.get(NodeId::new(2)).is_none());
        assert_eq!(tree.len(), 1); // only root remains
    }

    #[test]
    fn apply_entry_changed_missing_node_errors() {
        let mut tree = make_tree_with_root();

        let delta = WireTreeDelta::EntryChanged {
            id: NodeId::new(99), // doesn't exist
            status: WireNodeStatus::Ok,
            state: WireEntryState::Alive,
            change_frame: FrameId::new(5),
        };

        let result = apply_tree_delta(&mut tree, &delta, FrameId::new(5));
        assert!(result.is_err());
    }
}
