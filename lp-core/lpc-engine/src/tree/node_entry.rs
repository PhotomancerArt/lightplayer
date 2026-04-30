//! Per-instance metadata entry in the node tree.
//!
//! See `docs/roadmaps/2026-04-28-node-runtime/design/01-tree.md` §NodeEntry.

use alloc::vec::Vec;
use lpc_model::{FrameId, NodeId, TreePath};
use lpc_wire::{WireChildKind, WireNodeStatus};

use super::EntryState;

/// Server-side metadata for a node instance.
///
/// Generic over `N` — the payload type in `EntryState::Alive(N)`. In M3 this
/// is `()` (no Node trait yet). When the Node trait lands, this becomes
/// `Box<dyn Node>`.
///
/// Note: `Clone` is derived for testing; production usage doesn't require it.
#[derive(Clone, Debug)]
pub struct NodeEntry<N> {
    pub id: NodeId,
    pub path: TreePath,
    pub parent: Option<NodeId>,
    pub child_kind: Option<WireChildKind>, // None for root; immutable for entry's lifetime
    pub children: Vec<NodeId>,             // ordered

    pub status: WireNodeStatus,
    pub state: EntryState<N>,

    // Three frame counters per entry (12 bytes/entry); see design/01-tree.md
    // "Frame versioning" for why three (not five).
    pub created_frame: FrameId, // set on insert; never bumped
    pub change_frame: FrameId,  // bumped on status / state / (future: config) change
    pub children_ver: FrameId,  // bumped on children-list mutation

                                // Coming soon (separate plans uncomment + fill in):
                                // pub config:   lpc_model::NodeConfig,               // §design/04
                                // pub artifact: ArtifactRef,                         // §design/03
                                // pub prop_cache: BTreeMap<PropPath, ResolvedSlot>,  // §design/06
                                // pub prop_cache_ver: FrameId,                       // when editor watches live state
}

impl<N> NodeEntry<N> {
    /// Create a new entry. Sets `created_frame = change_frame = children_ver = frame`.
    pub fn new(
        id: NodeId,
        path: TreePath,
        parent: Option<NodeId>,
        child_kind: Option<WireChildKind>,
        frame: FrameId,
    ) -> Self {
        Self {
            id,
            path,
            parent,
            child_kind,
            children: Vec::new(),
            status: WireNodeStatus::Created,
            state: EntryState::Pending,
            created_frame: frame,
            change_frame: frame,
            children_ver: frame,
        }
    }

    /// Set status and bump `change_frame`.
    pub fn set_status(&mut self, status: WireNodeStatus, frame: FrameId) {
        self.status = status;
        self.change_frame = frame;
    }

    /// Set state and bump `change_frame`.
    pub fn set_state(&mut self, state: EntryState<N>, frame: FrameId) {
        self.state = state;
        self.change_frame = frame;
    }

    /// Returns true if this entry has any frame version newer than `since`.
    pub fn is_dirty_since(&self, since: FrameId) -> bool {
        self.created_frame.0 > since.0
            || self.change_frame.0 > since.0
            || self.children_ver.0 > since.0
    }
}

#[cfg(test)]
mod tests {
    use super::NodeEntry;
    use lpc_model::{FrameId, NodeId, TreePath};
    use lpc_wire::{WireChildKind, WireNodeStatus, WireSlotIndex};

    #[test]
    fn node_entry_new_sets_all_frame_counters() {
        let frame = FrameId::new(5);
        let entry: NodeEntry<()> = NodeEntry::new(
            NodeId::new(1),
            TreePath::parse("/main.show").unwrap(),
            None,
            None,
            frame,
        );
        assert_eq!(entry.created_frame.0, 5);
        assert_eq!(entry.change_frame.0, 5);
        assert_eq!(entry.children_ver.0, 5);
        assert_eq!(entry.status, WireNodeStatus::Created);
        assert!(entry.state.is_pending());
    }

    #[test]
    fn node_entry_set_status_bumps_change_frame() {
        let frame = FrameId::new(5);
        let mut entry: NodeEntry<()> = NodeEntry::new(
            NodeId::new(1),
            TreePath::parse("/main.show").unwrap(),
            None,
            None,
            frame,
        );
        entry.set_status(WireNodeStatus::Ok, FrameId::new(10));
        assert_eq!(entry.status, WireNodeStatus::Ok);
        assert_eq!(entry.change_frame.0, 10);
        // created_frame and children_ver unchanged
        assert_eq!(entry.created_frame.0, 5);
        assert_eq!(entry.children_ver.0, 5);
    }

    #[test]
    fn node_entry_is_dirty_since() {
        let frame = FrameId::new(5);
        let entry: NodeEntry<()> = NodeEntry::new(
            NodeId::new(1),
            TreePath::parse("/main.show").unwrap(),
            None,
            None,
            frame,
        );
        assert!(!entry.is_dirty_since(FrameId::new(5)));
        assert!(entry.is_dirty_since(FrameId::new(4)));
        assert!(!entry.is_dirty_since(FrameId::new(6)));
    }

    #[test]
    fn node_entry_child_kind_is_immutable_conceptually() {
        // Verify we can set it at construction; it's not changed after
        let frame = FrameId::new(1);
        let entry: NodeEntry<()> = NodeEntry::new(
            NodeId::new(2),
            TreePath::parse("/main.show/child.vis").unwrap(),
            Some(NodeId::new(1)),
            Some(WireChildKind::Input {
                source: WireSlotIndex(0),
            }),
            frame,
        );
        assert!(entry.child_kind.is_some());
        assert!(matches!(
            entry.child_kind,
            Some(WireChildKind::Input { .. })
        ));
    }
}
