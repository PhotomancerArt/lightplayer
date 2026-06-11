//! Effective project inventory.

use alloc::collections::BTreeMap;

use crate::{AssetEntry, AssetSource, NodeDefEntry, NodeDefLocation};

/// Effective post-overlay project state derived from artifacts plus overlay.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProjectInventory {
    pub defs: BTreeMap<NodeDefLocation, NodeDefEntry>,
    pub assets: BTreeMap<AssetSource, AssetEntry>,
}

impl ProjectInventory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty() && self.assets.is_empty()
    }
}
