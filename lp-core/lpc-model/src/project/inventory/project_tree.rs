use alloc::vec::Vec;
use lp_collection::VecMap;

use crate::{AssetLocation, NodeDefLocation, NodeUseLocation, ProjectNode};

/// Effective post-overlay project node uses and reverse indexes.
///
/// `ProjectTree` contains expanded node uses reachable from the project root.
/// It is tree-shaped because each use has one parent, even when multiple uses
/// point at the same [`crate::NodeDefLocation`].
///
/// Reverse indexes connect tree uses back to shared definitions and assets so
/// runtime projection can answer "which node uses this?" without re-walking
/// authored definitions.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectTree {
    /// Location of the project root use.
    pub root: NodeUseLocation,
    /// All effective node uses keyed by use location.
    pub nodes: VecMap<NodeUseLocation, ProjectNode>,
    /// Reverse index from definition location to node uses using it.
    pub def_instances: VecMap<NodeDefLocation, Vec<NodeUseLocation>>,
    /// Reverse index from asset source to node uses whose definitions reference it.
    pub asset_consumers: VecMap<AssetLocation, Vec<NodeUseLocation>>,
}

impl ProjectTree {
    pub fn new(root: NodeUseLocation) -> Self {
        Self {
            root,
            nodes: VecMap::new(),
            def_instances: VecMap::new(),
            asset_consumers: VecMap::new(),
        }
    }

    pub fn insert_node(&mut self, entry: ProjectNode) {
        self.def_instances
            .entry(entry.def_location.clone())
            .or_default()
            .push(entry.key.clone());
        self.nodes.insert(entry.key.clone(), entry);
    }

    pub fn add_asset_consumer(&mut self, source: AssetLocation, consumer: NodeUseLocation) {
        self.asset_consumers
            .entry(source)
            .or_default()
            .push(consumer);
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

impl Default for ProjectTree {
    fn default() -> Self {
        Self::new(NodeUseLocation::root())
    }
}
