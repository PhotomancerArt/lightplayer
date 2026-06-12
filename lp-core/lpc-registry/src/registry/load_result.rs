//! Result from loading the root project artifact.

use lpc_model::{NodeDefLocation, ProjectChangeSummary};

/// Initial effective inventory changes after loading a root project artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadResult {
    pub root: NodeDefLocation,
    pub changes: ProjectChangeSummary,
}

impl LoadResult {
    pub fn new(root: NodeDefLocation, changes: ProjectChangeSummary) -> Self {
        Self { root, changes }
    }
}
