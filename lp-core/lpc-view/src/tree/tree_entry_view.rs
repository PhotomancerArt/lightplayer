//! Local mirror of a node entry for tree sync (`NodeTreeView`).
//!
//! This type is distinct from [`NodeEntryView`](crate::project::NodeEntryView), which holds
//! full model config/state from `ProjectResponse`.
//!
//! See `docs/roadmaps/2026-04-28-node-runtime/design/07-sync.md`.

use alloc::vec::Vec;
use lpc_model::{FrameId, NodeId, TreePath};
use lpc_wire::{WireChildKind, WireEntryState, WireNodeStatus};

/// Mirror of wire/node tree metadata (`NodeEntry` on engine) without node payloads.
///
/// Holds the same metadata as the server-side entry but without the `Box<dyn Node>`
/// runtime object (the view does not execute nodes).
#[derive(Clone, Debug, PartialEq)]
pub struct TreeEntryView {
    pub id: NodeId,
    pub path: TreePath,
    pub parent: Option<NodeId>,
    pub child_kind: Option<WireChildKind>,
    pub children: Vec<NodeId>,

    pub status: WireNodeStatus,
    pub state: WireEntryState,

    pub created_frame: FrameId,
    pub change_frame: FrameId,
    pub children_ver: FrameId,
    // Coming soon (mirrors NodeEntry future fields):
    // pub config: NodeConfig,
    // pub prop_cache: BTreeMap<PropPath, (LpsValue, FrameId)>,
    // pub prop_cache_ver: FrameId,
}

impl TreeEntryView {
    /// Create a new entry from components (used during delta application).
    pub fn new(
        id: NodeId,
        path: TreePath,
        parent: Option<NodeId>,
        child_kind: Option<WireChildKind>,
        status: WireNodeStatus,
        state: WireEntryState,
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
