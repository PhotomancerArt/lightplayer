use alloc::vec::Vec;

use crate::{NodeDefChangeDetail, NodeDefLocation, NodeDefUpdates};

/// Portable commit summary.
///
/// Commit summaries describe definition-level effects of a commit in a compact
/// form that can be sent to clients or logged without carrying full inventory
/// snapshots.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectCommitSummary {
    /// Added, changed, and removed definition locations.
    pub def_updates: NodeDefUpdates,
    /// More detailed classification for changed definitions.
    pub change_details: Vec<(NodeDefLocation, NodeDefChangeDetail)>,
}

impl ProjectCommitSummary {
    pub fn is_empty(&self) -> bool {
        self.def_updates.is_empty() && self.change_details.is_empty()
    }
}
