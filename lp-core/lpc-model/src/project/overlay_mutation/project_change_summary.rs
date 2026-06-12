use crate::{AssetChangeSummary, NodeDefChangeSummary, NodeUseChangeSummary};

/// Runtime-facing summary of changes from one effective project inventory to another.
///
/// A project change summary is a compact description of how one effective
/// [`crate::ProjectInventory`] differs from another. It is intended for runtime
/// projection and client refresh decisions, not for reconstructing an overlay.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectChangeSummary {
    /// Node definition additions, removals, and changes.
    pub defs: NodeDefChangeSummary,
    /// Asset additions, removals, and changes.
    pub assets: AssetChangeSummary,
    /// Node-use additions, removals, and structural changes.
    pub uses: NodeUseChangeSummary,
}

impl ProjectChangeSummary {
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty() && self.assets.is_empty() && self.uses.is_empty()
    }
}
