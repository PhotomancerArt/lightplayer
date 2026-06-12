use alloc::collections::BTreeMap;

use crate::{AssetEntry, AssetSource, NodeDefEntry, NodeDefLocation, ProjectTree};

/// Effective post-overlay project state derived from artifacts plus overlay.
///
/// `ProjectInventory` is the complete effective project read model. It contains
/// both unique referenced things (`defs`, `assets`) and the expanded project
/// node tree (`tree`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProjectInventory {
    /// Unique referenced node definitions keyed by definition location.
    pub defs: BTreeMap<NodeDefLocation, NodeDefEntry>,
    /// Unique referenced assets keyed by asset source.
    pub assets: BTreeMap<AssetSource, AssetEntry>,
    /// Expanded effective node uses reachable from the project root.
    pub tree: ProjectTree,
}

impl ProjectInventory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty() && self.assets.is_empty() && self.tree.is_empty()
    }
}
