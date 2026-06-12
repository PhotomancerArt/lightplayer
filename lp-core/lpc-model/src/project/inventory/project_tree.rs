//! Effective project graph topology.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::{AssetSource, NodeDefLocation, ProjectNode, ProjectNodeLocation};

/// Effective post-overlay project node graph and reverse indexes.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectTree {
    pub root: ProjectNodeLocation,
    pub nodes: BTreeMap<ProjectNodeLocation, ProjectNode>,
    pub def_instances: BTreeMap<NodeDefLocation, Vec<ProjectNodeLocation>>,
    pub asset_consumers: BTreeMap<AssetSource, Vec<ProjectNodeLocation>>,
}

impl ProjectTree {
    pub fn new(root: ProjectNodeLocation) -> Self {
        Self {
            root,
            nodes: BTreeMap::new(),
            def_instances: BTreeMap::new(),
            asset_consumers: BTreeMap::new(),
        }
    }

    pub fn insert_node(&mut self, entry: ProjectNode) {
        self.def_instances
            .entry(entry.def_location.clone())
            .or_default()
            .push(entry.key.clone());
        self.nodes.insert(entry.key.clone(), entry);
    }

    pub fn add_asset_consumer(&mut self, source: AssetSource, consumer: ProjectNodeLocation) {
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
        Self::new(ProjectNodeLocation::root())
    }
}
