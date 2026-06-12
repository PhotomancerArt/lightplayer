use crate::{ArtifactChangeSet, ProjectChangeSet};

/// Result from committing pending [`crate::ProjectOverlay`] edits to artifacts.
///
/// Commit is the point where overlay intent is persisted back to artifact
/// storage. This reports both filesystem-level artifact changes and effective
/// project inventory changes observed after the commit.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommitResult {
    /// Artifact bodies that were added, changed, or removed on durable storage.
    pub artifacts: ArtifactChangeSet,
    /// Effective node definition and asset changes after the commit.
    pub changes: ProjectChangeSet,
}

impl CommitResult {
    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty() && self.changes.is_empty()
    }
}
