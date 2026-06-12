use crate::ArtifactChangeSummary;

/// Result from committing pending [`crate::ProjectOverlay`] edits to artifacts.
///
/// Commit is the point where overlay intent is persisted back to artifact
/// storage. This reports both filesystem-level artifact changes and effective
/// project inventory changes observed after the commit.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommitResult {
    /// Artifact bodies that were added, changed, or removed on durable storage.
    pub artifact_changes: ArtifactChangeSummary,
}

impl CommitResult {
    pub fn is_empty(&self) -> bool {
        self.artifact_changes.is_empty()
    }
}
