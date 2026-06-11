//! Effective project inventory changes.

use crate::{AssetChangeSet, NodeDefChangeSet};

/// Runtime-facing changes from one effective project inventory to another.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectChangeSet {
    pub defs: NodeDefChangeSet,
    pub assets: AssetChangeSet,
}

impl ProjectChangeSet {
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty() && self.assets.is_empty()
    }
}
