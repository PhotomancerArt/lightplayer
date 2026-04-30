//! Client-side mirror of a node entry.
//!
//! Named `ClientTreeEntry` to avoid collision with the legacy `ClientNodeEntry`
//! in `project::view`.
//!
//! See `docs/roadmaps/2026-04-28-node-runtime/design/07-sync.md`.

use alloc::vec::Vec;
use lpc_model::project::api::NodeStatus;
use lpc_model::{ChildKind, EntryStateView, FrameId, NodeId, TreePath};

/// Client-side mirror of a `NodeEntry`.
///
/// Holds the same metadata as the server-side entry but without the `Box<dyn Node>`
/// payload (the client doesn't run nodes).
#[derive(Clone, Debug, PartialEq)]
pub struct ClientTreeEntry {
    pub id: NodeId,
    pub path: TreePath,
    pub parent: Option<NodeId>,
    pub child_kind: Option<ChildKind>,
    pub children: Vec<NodeId>,

    pub status: NodeStatus,
    pub state: EntryStateView,

    pub created_frame: FrameId,
    pub change_frame: FrameId,
    pub children_ver: FrameId,
    // Coming soon (mirrors NodeEntry future fields):
    // pub config: NodeConfig,
    // pub prop_cache: BTreeMap<PropPath, (LpsValue, FrameId)>,
    // pub prop_cache_ver: FrameId,
}

impl ClientTreeEntry {
    /// Create a new entry from components (used during delta application).
    pub fn new(
        id: NodeId,
        path: TreePath,
        parent: Option<NodeId>,
        child_kind: Option<ChildKind>,
        status: NodeStatus,
        state: EntryStateView,
        created_frame: FrameId,
        change_frame: FrameId,
        children_ver: FrameId,
    ) -> Self {
        Self {
            id,
            path,
            parent,
            child_kind,
            children: Vec::new(),
            status,
            state,
            created_frame,
            change_frame,
            children_ver,
        }
    }
}
