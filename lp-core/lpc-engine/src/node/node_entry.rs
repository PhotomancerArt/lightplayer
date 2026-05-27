//! Per-instance metadata entry in the node tree.
//!
//! See `docs/roadmaps/2026-04-28-node-runtime/design/01-tree.md` §NodeEntry.

use alloc::vec::Vec;
use lpc_model::{ArtifactSpec, NodeId, NodeInvocation, Revision, TreePath, WithRevision};
use lpc_wire::{WireChildKind, WireNodeStatus};

use crate::artifact::ArtifactId;
use crate::dataflow::binding::BindingSet;
use crate::node::node_entry_state::NodeEntryState;

use super::NodeDefHandle;

/// Server-side metadata for a node instance.
///
/// Generic over `N` — the payload type in `EntryState::Alive(N)`. In M3 this
/// is `()` (no Node trait yet). When the Node trait lands, this becomes
/// `Box<dyn Node>`.
///
#[derive(Debug)]
pub struct NodeEntry<N> {
    pub id: NodeId,
    pub path: TreePath,
    pub parent: Option<NodeId>,
    pub child_kind: Option<WireChildKind>, // None for root; immutable for entry's lifetime
    pub children: WithRevision<Vec<NodeId>>, // ordered

    pub status: WithRevision<WireNodeStatus>,
    pub state: WithRevision<NodeEntryState<N>>,
    pub bindings: WithRevision<BindingSet>,

    pub created_at: Revision,

    /// Authored per-instance config (artifact spec + overrides).
    pub config: NodeInvocation,

    /// Runtime handle to this node's authored definition.
    pub def_handle: NodeDefHandle,
}

impl<N> NodeEntry<N> {
    /// Placeholder artifact path for [`Self::new`] (tests and roots without a real spec yet).
    ///
    /// Spine placeholder artifact path: empty authored `""` normalizes to `/` (`lpc_model::LpPathBuf`).
    pub(crate) const PLACEHOLDER_ARTIFACT_PATH: &'static str = "/";

    /// Create a new entry. Sets `created_at`, `changed_at`, and
    /// `children_changed_at` to `revision`.
    ///
    /// Fills spine fields with placeholders: root-normalized artifact path (`/`), handle `0`.
    pub fn new(
        id: NodeId,
        path: TreePath,
        parent: Option<NodeId>,
        child_kind: Option<WireChildKind>,
        revision: Revision,
    ) -> Self {
        Self::new_spine(
            id,
            path,
            parent,
            child_kind,
            NodeInvocation::new(ArtifactSpec::path(Self::PLACEHOLDER_ARTIFACT_PATH)),
            NodeDefHandle::artifact_root(ArtifactId::from_raw(0)),
            revision,
        )
    }

    /// Create a new entry with explicit source config and artifact handle.
    pub fn new_spine(
        id: NodeId,
        path: TreePath,
        parent: Option<NodeId>,
        child_kind: Option<WireChildKind>,
        config: NodeInvocation,
        def_handle: NodeDefHandle,
        revision: Revision,
    ) -> Self {
        Self {
            id,
            path,
            parent,
            child_kind,
            children: WithRevision::new(revision, Vec::new()),
            status: WithRevision::new(revision, WireNodeStatus::Created),
            state: WithRevision::new(revision, NodeEntryState::Pending),
            bindings: WithRevision::new(revision, BindingSet::new()),
            created_at: revision,
            config,
            def_handle,
        }
    }

    pub fn artifact(&self) -> ArtifactId {
        self.def_handle.artifact()
    }

    /// Set status and bump `changed_at`.
    pub fn set_status(&mut self, status: WireNodeStatus, revision: Revision) {
        self.status.set(revision, status);
    }

    /// Set state and bump `changed_at`.
    pub fn set_state(&mut self, state: NodeEntryState<N>, revision: Revision) {
        self.state.set(revision, state);
    }

    /// The latest revision for this entry's non-structural metadata.
    pub fn changed_at(&self) -> Revision {
        core::cmp::max(self.status.changed_at(), self.state.changed_at())
    }

    /// The latest revision for this entry's ordered child list.
    pub fn children_changed_at(&self) -> Revision {
        self.children.changed_at()
    }

    /// Returns true if this entry has any revision marker newer than `since`.
    pub fn is_dirty_since(&self, since: Revision) -> bool {
        self.created_at.0 > since.0
            || self.changed_at().0 > since.0
            || self.children_changed_at().0 > since.0
    }
}

#[cfg(test)]
mod tests {
    use super::NodeEntry;
    use crate::node::NodeDefHandle;
    use lpc_model::{ArtifactSpec, NodeInvocation};
    use lpc_model::{NodeId, Revision, TreePath};
    use lpc_wire::{WireChildKind, WireNodeStatus, WireSlotIndex};

    #[test]
    fn node_entry_new_sets_all_frame_counters() {
        let frame = Revision::new(5);
        let entry: NodeEntry<()> = NodeEntry::new(
            NodeId::new(1),
            TreePath::parse("/main.show").unwrap(),
            None,
            None,
            frame,
        );
        assert_eq!(entry.created_at.0, 5);
        assert_eq!(entry.changed_at().0, 5);
        assert_eq!(entry.children_changed_at().0, 5);
        assert_eq!(*entry.status.value(), WireNodeStatus::Created);
        assert!(entry.state.value().is_pending());
    }

    #[test]
    fn node_entry_set_status_bumps_change_frame() {
        let frame = Revision::new(5);
        let mut entry: NodeEntry<()> = NodeEntry::new(
            NodeId::new(1),
            TreePath::parse("/main.show").unwrap(),
            None,
            None,
            frame,
        );
        entry.set_status(WireNodeStatus::Ok, Revision::new(10));
        assert_eq!(*entry.status.value(), WireNodeStatus::Ok);
        assert_eq!(entry.changed_at().0, 10);
        // created_frame and children_ver unchanged
        assert_eq!(entry.created_at.0, 5);
        assert_eq!(entry.children_changed_at().0, 5);
    }

    #[test]
    fn node_entry_is_dirty_since() {
        let frame = Revision::new(5);
        let entry: NodeEntry<()> = NodeEntry::new(
            NodeId::new(1),
            TreePath::parse("/main.show").unwrap(),
            None,
            None,
            frame,
        );
        assert!(!entry.is_dirty_since(Revision::new(5)));
        assert!(entry.is_dirty_since(Revision::new(4)));
        assert!(!entry.is_dirty_since(Revision::new(6)));
    }

    #[test]
    fn node_entry_child_kind_is_immutable_conceptually() {
        // Verify we can set it at construction; it's not changed after
        let frame = Revision::new(1);
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

    #[test]
    fn node_entry_new_spine_stores_config_and_def_handle() {
        let frame = Revision::new(1);
        let config = NodeInvocation::new(ArtifactSpec::path("./fluid.vis"));
        let artifact = crate::artifact::ArtifactId::from_raw(7);
        let def_handle = NodeDefHandle::artifact_root(artifact);
        let entry: NodeEntry<()> = NodeEntry::new_spine(
            NodeId::new(1),
            TreePath::parse("/main.show").unwrap(),
            None,
            None,
            config.clone(),
            def_handle.clone(),
            frame,
        );
        assert_eq!(entry.config, config);
        assert_eq!(entry.def_handle, def_handle);
        assert_eq!(entry.artifact(), artifact);
    }
}
