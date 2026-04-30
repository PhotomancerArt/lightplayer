//! Structural tree deltas for engine↔client sync (`WireTreeDelta`).

use crate::project::WireNodeStatus;
use crate::tree::{WireChildKind, WireEntryState};
use lpc_model::node::{NodeId, TreePath};
use lpc_model::project::FrameId;

/// Structural delta for the node tree (wire shape).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(tag = "delta", rename_all = "snake_case")]
pub enum WireTreeDelta {
    /// First time the client sees this entry.
    Created {
        id: NodeId,
        path: TreePath,
        parent: Option<NodeId>,
        child_kind: Option<WireChildKind>,
        children: alloc::vec::Vec<NodeId>,
        status: WireNodeStatus,
        state: WireEntryState,
        created_frame: FrameId,
        change_frame: FrameId,
        children_ver: FrameId,
    },

    /// Status/state changed on an existing entry.
    EntryChanged {
        id: NodeId,
        status: WireNodeStatus,
        state: WireEntryState,
        change_frame: FrameId,
    },

    /// Children list changed.
    ChildrenChanged {
        id: NodeId,
        children: alloc::vec::Vec<NodeId>,
        children_ver: FrameId,
    },
}

#[cfg(test)]
mod tests {
    use super::WireTreeDelta;
    use crate::project::WireNodeStatus;
    use crate::tree::{SlotIdx, WireChildKind, WireEntryState};
    use lpc_model::node::{NodeId, TreePath};
    use lpc_model::project::FrameId;

    #[test]
    fn tree_delta_created_round_trips() {
        let delta = WireTreeDelta::Created {
            id: NodeId::new(7),
            path: TreePath::parse("/main.show/fluid.vis").unwrap(),
            parent: Some(NodeId::new(1)),
            child_kind: Some(WireChildKind::Input { source: SlotIdx(0) }),
            children: alloc::vec![NodeId::new(8), NodeId::new(9)],
            status: WireNodeStatus::Created,
            state: WireEntryState::Pending,
            created_frame: FrameId::new(1),
            change_frame: FrameId::new(1),
            children_ver: FrameId::new(1),
        };
        let json = serde_json::to_string(&delta).unwrap();
        let decoded: WireTreeDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(delta, decoded);
    }

    #[test]
    fn tree_delta_entry_changed_round_trips() {
        let delta = WireTreeDelta::EntryChanged {
            id: NodeId::new(7),
            status: WireNodeStatus::Ok,
            state: WireEntryState::Alive,
            change_frame: FrameId::new(42),
        };
        let json = serde_json::to_string(&delta).unwrap();
        let decoded: WireTreeDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(delta, decoded);
    }

    #[test]
    fn tree_delta_children_changed_round_trips() {
        let delta = WireTreeDelta::ChildrenChanged {
            id: NodeId::new(1),
            children: alloc::vec![NodeId::new(2), NodeId::new(3)],
            children_ver: FrameId::new(10),
        };
        let json = serde_json::to_string(&delta).unwrap();
        let decoded: WireTreeDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(delta, decoded);
    }

    #[test]
    fn tree_delta_root_created_has_none_child_kind() {
        let delta = WireTreeDelta::Created {
            id: NodeId::new(0),
            path: TreePath::parse("/root.show").unwrap(),
            parent: None,
            child_kind: None,
            children: alloc::vec![],
            status: WireNodeStatus::Created,
            state: WireEntryState::Pending,
            created_frame: FrameId::new(0),
            change_frame: FrameId::new(0),
            children_ver: FrameId::new(0),
        };
        let json = serde_json::to_string(&delta).unwrap();
        let decoded: WireTreeDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(delta, decoded);
        if let WireTreeDelta::Created {
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

    #[test]
    fn wire_project_handle_round_trips_via_json_wrapper() {
        let h = crate::project::WireProjectHandle::new(42);
        let json = crate::json::to_string(&h).unwrap();
        let back: crate::project::WireProjectHandle = crate::json::from_str(&json).unwrap();
        assert_eq!(h, back);
    }
}
