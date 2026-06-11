//! Portable project commit summaries.

use alloc::vec::Vec;

use crate::{NodeDefChangeDetail, NodeDefLocation, NodeDefUpdates};

/// Portable commit summary.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectCommitSummary {
    pub def_updates: NodeDefUpdates,
    pub change_details: Vec<(NodeDefLocation, NodeDefChangeDetail)>,
}

impl ProjectCommitSummary {
    pub fn is_empty(&self) -> bool {
        self.def_updates.is_empty() && self.change_details.is_empty()
    }
}
