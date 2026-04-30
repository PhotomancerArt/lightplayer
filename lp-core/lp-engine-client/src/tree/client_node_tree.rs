//! Client-side mirror of the node tree.
//!
//! See `docs/roadmaps/2026-04-28-node-runtime/design/07-sync.md`.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use lpc_model::{FrameId, NodeId, TreePath};

use super::ClientTreeEntry;

/// Client-side mirror of the node tree.
///
/// Maintained by applying `WireTreeDelta`s from the server.
#[derive(Clone, Debug)]
pub struct ClientNodeTree {
    pub nodes: BTreeMap<NodeId, ClientTreeEntry>,
    pub by_path: BTreeMap<TreePath, NodeId>,
    pub last_synced_frame: FrameId,
}

impl ClientNodeTree {
    /// Create an empty tree.
    pub fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
            by_path: BTreeMap::new(),
            last_synced_frame: FrameId::new(0),
        }
    }

    /// Get a reference to an entry by id.
    pub fn get(&self, id: NodeId) -> Option<&ClientTreeEntry> {
        self.nodes.get(&id)
    }

    /// Get a mutable reference to an entry by id.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut ClientTreeEntry> {
        self.nodes.get_mut(&id)
    }

    /// Look up a node by its path.
    pub fn lookup_path(&self, path: &TreePath) -> Option<NodeId> {
        self.by_path.get(path).copied()
    }

    /// Insert an entry (used during delta application).
    pub fn insert(&mut self, entry: ClientTreeEntry) {
        self.by_path.insert(entry.path.clone(), entry.id);
        self.nodes.insert(entry.id, entry);
    }

    /// Remove an entry and its descendants (used during inferred removal).
    ///
    /// Returns the number of entries removed.
    pub fn remove_subtree(&mut self, id: NodeId) -> usize {
        let mut count = 0;

        // Collect descendants to remove (depth-first)
        if let Some(entry) = self.nodes.get(&id) {
            let descendants: Vec<NodeId> = entry.children.clone();
            for child_id in descendants {
                count += self.remove_subtree(child_id);
            }
        }

        // Remove this entry
        if let Some(entry) = self.nodes.remove(&id) {
            self.by_path.remove(&entry.path);
            count += 1;
        }

        count
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
    pub fn update_synced_frame(&mut self, frame: FrameId) {
        if frame.0 > self.last_synced_frame.0 {
            self.last_synced_frame = frame;
        }
    }
}

impl Default for ClientNodeTree {
    fn default() -> Self {
        Self::new()
    }
}
