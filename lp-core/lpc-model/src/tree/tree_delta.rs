//! Domain-agnostic structural tree deltas for client/server sync.
//!
//! These deltas are **shape-only**: they carry tree structure (parent/child
//! relationships, status, entry state) but no domain-specific payload like
//! `NodeConfig` or per-prop values. Domain responses wrap `TreeDelta` with
//! their own extras.
//!
//! See `docs/roadmaps/2026-04-28-node-runtime/design/07-sync.md`.

use crate::node::{NodeId, TreePath};
use crate::project::FrameId;
use crate::project::api::NodeStatus;

use super::{ChildKind, EntryStateView};

/// Structural delta for the node tree.
///
/// **No `Destroyed` variant.** Clients infer removals by diffing the new
/// children list against their mirror on `ChildrenChanged`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(tag = "delta", rename_all = "snake_case")]
pub enum TreeDelta {
    /// New entry (first time client sees it).
    Created {
        id: NodeId,
        path: TreePath,
        parent: Option<NodeId>,
        child_kind: Option<ChildKind>,
        children: alloc::vec::Vec<NodeId>,
        status: NodeStatus,
        state: EntryStateView,
        created_frame: FrameId,
        change_frame: FrameId,
        children_ver: FrameId,
        // Coming soon (NodeConfig plan):
        // config: NodeConfig,
    },

    /// Existing entry's status / state / (future: config) changed.
    EntryChanged {
        id: NodeId,
        status: NodeStatus,
        state: EntryStateView,
        change_frame: FrameId,
        // Coming soon (NodeConfig plan):
        // config: Option<NodeConfig>,
    },

    /// Children list mutated (insert, remove, reorder). Client infers
    /// removals by diffing against its mirror.
    ChildrenChanged {
        id: NodeId,
        children: alloc::vec::Vec<NodeId>,
        children_ver: FrameId,
    },
    // Coming soon (per-prop deltas; wired when editor demands live-state watching):
    // PropsChanged {
    //     id: NodeId,
    //     entries: Vec<(PropPath, LpsValue)>,
    //     prop_cache_ver: FrameId,
    // },
}

#[cfg(test)]
mod tests {
    use super::TreeDelta;
    use crate::node::{NodeId, TreePath};
    use crate::project::FrameId;
    use crate::project::api::NodeStatus;
    use crate::tree::{ChildKind, EntryStateView, SlotIdx};

    #[test]
    fn tree_delta_created_round_trips() {
        let delta = TreeDelta::Created {
            id: NodeId::new(7),
            path: TreePath::parse("/main.show/fluid.vis").unwrap(),
            parent: Some(NodeId::new(1)),
            child_kind: Some(ChildKind::Input { source: SlotIdx(0) }),
            children: alloc::vec![NodeId::new(8), NodeId::new(9)],
            status: NodeStatus::Created,
            state: EntryStateView::Pending,
            created_frame: FrameId::new(1),
            change_frame: FrameId::new(1),
            children_ver: FrameId::new(1),
        };
        let json = serde_json::to_string(&delta).unwrap();
        let decoded: TreeDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(delta, decoded);
    }

    #[test]
    fn tree_delta_entry_changed_round_trips() {
        let delta = TreeDelta::EntryChanged {
            id: NodeId::new(7),
            status: NodeStatus::Ok,
            state: EntryStateView::Alive,
            change_frame: FrameId::new(42),
        };
        let json = serde_json::to_string(&delta).unwrap();
        let decoded: TreeDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(delta, decoded);
    }

    #[test]
    fn tree_delta_children_changed_round_trips() {
        let delta = TreeDelta::ChildrenChanged {
            id: NodeId::new(1),
            children: alloc::vec![NodeId::new(2), NodeId::new(3)],
            children_ver: FrameId::new(10),
        };
        let json = serde_json::to_string(&delta).unwrap();
        let decoded: TreeDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(delta, decoded);
    }

    #[test]
    fn tree_delta_root_created_has_none_child_kind() {
        // Root has parent=None and child_kind=None, empty children.
        // Note: NodePath must have at least one segment ("/" alone is Empty).
        let delta = TreeDelta::Created {
            id: NodeId::new(0),
            path: TreePath::parse("/root.show").unwrap(),
            parent: None,
            child_kind: None,
            children: alloc::vec![],
            status: NodeStatus::Created,
            state: EntryStateView::Pending,
            created_frame: FrameId::new(0),
            change_frame: FrameId::new(0),
            children_ver: FrameId::new(0),
        };
        let json = serde_json::to_string(&delta).unwrap();
        let decoded: TreeDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(delta, decoded);
        // Verify the parent and child_kind are None (root markers)
        if let TreeDelta::Created {
            parent,
            child_kind,
            children,
            ..
        } = decoded
        {
            assert!(parent.is_none());
            assert!(child_kind.is_none());
            assert!(children.is_empty());
        } else {
            panic!("Expected Created delta");
        }
    }
}
