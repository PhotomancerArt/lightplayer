//! Client-side mirror of the node tree.
//!
//! See `docs/roadmaps/2026-04-28-node-runtime/design/07-sync.md`.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use lpc_model::{NodeId, Revision, TreePath};

use super::TreeEntryView;

/// Client-side mirror of the node tree.
///
/// Maintained by applying `WireTreeDelta`s from the server.
#[derive(Clone, Debug)]
pub struct NodeTreeView {
    pub nodes: BTreeMap<NodeId, TreeEntryView>,
    pub by_path: BTreeMap<TreePath, NodeId>,
    pub last_synced_frame: Revision,
}

impl NodeTreeView {
    /// Create an empty tree.
    pub fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
            by_path: BTreeMap::new(),
            last_synced_frame: Revision::new(0),
        }
    }

    /// Get a reference to an entry by id.
    pub fn get(&self, id: NodeId) -> Option<&TreeEntryView> {
        self.nodes.get(&id)
    }

    /// Get a mutable reference to an entry by id.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut TreeEntryView> {
        self.nodes.get_mut(&id)
    }

    /// Look up a node by its path.
    pub fn lookup_path(&self, path: &TreePath) -> Option<NodeId> {
        self.by_path.get(path).copied()
    }

    /// Insert an entry (used during delta application).
    pub fn insert(&mut self, entry: TreeEntryView) {
        self.by_path.insert(entry.path.clone(), entry.id);
        self.nodes.insert(entry.id, entry);
    }

    /// Remove an entry and its descendants (used during inferred removal).
    ///
    /// Returns the number of entries removed.
    pub fn remove_subtree(&mut self, id: NodeId) -> usize {
        let mut removed = Vec::new();
        self.remove_subtree_collecting(id, &mut removed);
        removed.len()
    }

    /// Remove an entry and its descendants, pushing every removed node id into
    /// `removed` (depth-first).
    ///
    /// Used by the project-read apply path so slot roots owned by removed nodes
    /// can be dropped from the slot mirror.
    pub fn remove_subtree_collecting(&mut self, id: NodeId, removed: &mut Vec<NodeId>) {
        // Collect descendants to remove (depth-first)
        if let Some(entry) = self.nodes.get(&id) {
            let descendants: Vec<NodeId> = entry.children.clone();
            for child_id in descendants {
                self.remove_subtree_collecting(child_id, removed);
            }
        }

        // Remove this entry
        if let Some(entry) = self.nodes.remove(&id) {
            self.by_path.remove(&entry.path);
            removed.push(id);
        }
    }

    /// Returns true if the tree has no entries.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Returns the number of entries in the tree.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Update the last synced frame (call after applying deltas).
    pub fn update_synced_frame(&mut self, frame: Revision) {
        if frame.0 > self.last_synced_frame.0 {
            self.last_synced_frame = frame;
        }
    }
}

impl Default for NodeTreeView {
    fn default() -> Self {
        Self::new()
    }
}
