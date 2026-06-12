//! Effective project graph topology.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::{AssetSource, NodeDefLocation, ProjectNodeEntry, ProjectNodeKey};

/// Effective post-overlay project node graph and reverse indexes.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectGraph {
    pub root: ProjectNodeKey,
    pub nodes: BTreeMap<ProjectNodeKey, ProjectNodeEntry>,
    pub def_instances: BTreeMap<NodeDefLocation, Vec<ProjectNodeKey>>,
    pub asset_consumers: BTreeMap<AssetSource, Vec<ProjectNodeKey>>,
}

impl ProjectGraph {
    pub fn new(root: ProjectNodeKey) -> Self {
        Self {
            root,
            nodes: BTreeMap::new(),
            def_instances: BTreeMap::new(),
            asset_consumers: BTreeMap::new(),
        }
    }

    pub fn insert_node(&mut self, entry: ProjectNodeEntry) {
        self.def_instances
            .entry(entry.def_location.clone())
            .or_default()
            .push(entry.key.clone());
        self.nodes.insert(entry.key.clone(), entry);
    }

    pub fn add_asset_consumer(&mut self, source: AssetSource, consumer: ProjectNodeKey) {
        self.asset_consumers
            .entry(source)
            .or_default()
            .push(consumer);
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

impl Default for ProjectGraph {
    fn default() -> Self {
        Self::new(ProjectNodeKey::root())
    }
}
