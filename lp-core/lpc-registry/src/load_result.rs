//! Result from loading the root project artifact.

use lpc_model::{NodeDefLocation, ProjectChangeSet};

/// Initial effective inventory changes after loading a root project artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadResult {
    pub root: NodeDefLocation,
    pub changes: ProjectChangeSet,
}

impl LoadResult {
    pub fn new(root: NodeDefLocation, changes: ProjectChangeSet) -> Self {
        Self { root, changes }
    }
}
