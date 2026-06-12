use crate::{AssetChangeSet, NodeDefChangeSet};

/// Runtime-facing changes from one effective project inventory to another.
///
/// A project change set is a compact description of how one effective
/// [`crate::ProjectInventory`] differs from another. It is intended for runtime
/// projection and client refresh decisions, not for reconstructing an overlay.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectChangeSet {
    /// Node definition additions, removals, and changes.
    pub defs: NodeDefChangeSet,
    /// Asset additions, removals, and changes.
    pub assets: AssetChangeSet,
}

impl ProjectChangeSet {
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty() && self.assets.is_empty()
    }
}
