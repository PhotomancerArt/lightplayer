//! Result from committing project overlay edits to durable artifacts.

use crate::{ArtifactChangeSet, ProjectChangeSet};

/// Persistence result plus any effective project changes after commit.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommitResult {
    pub artifacts: ArtifactChangeSet,
    pub changes: ProjectChangeSet,
}

impl CommitResult {
    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty() && self.changes.is_empty()
    }
}
