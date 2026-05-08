//! Source authoring snapshots keyed by runtime node id.
//!
//! The core engine tree stores runtime nodes as [`crate::node::NodeRuntime`] trait objects without a
//! direct source-definition owner. The loader captures the typed definitions it read from disk and
//! indexes them here keyed by [`lpc_model::NodeId`] so canonical project sync can expose source
//! slot roots.

use hashbrown::HashMap;

use lpc_model::{LpPathBuf, NodeId};

use super::project_loader::LoadedNodeDef;

/// Source definition/path index for loaded runtime nodes.
pub struct SourceAuthoringIndex {
    authoring_defs: HashMap<NodeId, LoadedNodeDef>,
    authoring_paths: HashMap<NodeId, LpPathBuf>,
}

impl SourceAuthoringIndex {
    pub fn new() -> Self {
        Self {
            authoring_defs: HashMap::new(),
            authoring_paths: HashMap::new(),
        }
    }

    pub(super) fn record_authoring_snapshot(
        &mut self,
        id: NodeId,
        path: LpPathBuf,
        def: LoadedNodeDef,
    ) {
        self.authoring_defs.insert(id, def);
        self.authoring_paths.insert(id, path);
    }

    pub fn node_def_for(&self, id: NodeId) -> Option<&LoadedNodeDef> {
        self.authoring_defs.get(&id)
    }

    pub fn node_path_for(&self, id: NodeId) -> Option<&LpPathBuf> {
        self.authoring_paths.get(&id)
    }
}

impl Default for SourceAuthoringIndex {
    fn default() -> Self {
        Self::new()
    }
}
