//! Compatibility authoring snapshots for M4.1 wire/detail projection.
//!
//! The core engine tree stores runtime nodes as [`crate::node::Node`] trait objects without a
//! stable way to recover legacy [`lpc_source::legacy::nodes::NodeConfig`] clones. The loader
//! captures the typed configs it read from disk and indexes them here keyed by [`lpc_model::NodeId`].

use alloc::boxed::Box;
use hashbrown::HashMap;

use lpc_model::{LpPathBuf, NodeId};
use lpc_source::legacy::nodes::NodeConfig;

use super::project_loader::LoadedNodeConfig;

/// Authoring/config index for legacy-compatible [`lpc_wire::legacy::NodeDetail`] construction.
pub struct CompatibilityProjection {
    authoring_configs: HashMap<NodeId, LoadedNodeConfig>,
    authoring_paths: HashMap<NodeId, LpPathBuf>,
}

impl CompatibilityProjection {
    pub fn new() -> Self {
        Self {
            authoring_configs: HashMap::new(),
            authoring_paths: HashMap::new(),
        }
    }

    pub(super) fn record_authoring_snapshot(
        &mut self,
        id: NodeId,
        path: LpPathBuf,
        cfg: LoadedNodeConfig,
    ) {
        self.authoring_configs.insert(id, cfg);
        self.authoring_paths.insert(id, path);
    }

    pub fn node_config_box_for(&self, id: NodeId) -> Option<Box<dyn NodeConfig>> {
        self.authoring_configs
            .get(&id)
            .map(LoadedNodeConfig::clone_as_node_config_box)
    }

    pub fn node_path_for(&self, id: NodeId) -> Option<&LpPathBuf> {
        self.authoring_paths.get(&id)
    }
}

impl Default for CompatibilityProjection {
    fn default() -> Self {
        Self::new()
    }
}
