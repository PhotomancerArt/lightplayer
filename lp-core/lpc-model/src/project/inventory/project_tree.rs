use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::{AssetSource, NodeDefLocation, ProjectNode, ProjectNodeLocation};

/// Effective post-overlay project node occurrences and reverse indexes.
///
/// `ProjectTree` contains expanded node occurrences reachable from the project
/// root. It is tree-shaped because each occurrence has one parent, even when
/// multiple occurrences point at the same [`crate::NodeDefLocation`].
///
/// Reverse indexes connect tree occurrences back to shared definitions and
/// assets so runtime projection can answer "which node occurrences use this?"
/// without re-walking authored definitions.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectTree {
    /// Location of the project root occurrence.
    pub root: ProjectNodeLocation,
    /// All effective node occurrences keyed by project-node location.
    pub nodes: BTreeMap<ProjectNodeLocation, ProjectNode>,
    /// Reverse index from definition location to node occurrences using it.
    pub def_instances: BTreeMap<NodeDefLocation, Vec<ProjectNodeLocation>>,
    /// Reverse index from asset source to node occurrences whose defs reference it.
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
