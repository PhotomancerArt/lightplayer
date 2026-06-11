//! Project-level edit batches and portable results.

use alloc::string::String;
use alloc::vec::Vec;

use crate::edit::{ArtifactEdit, DefinitionLocation};
use crate::{LpPathBuf, NodeKind};

/// Client-visible id for one project edit command.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    Ord,
    PartialOrd,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(transparent)]
pub struct ProjectEditCommandId(pub u64);

impl ProjectEditCommandId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    pub const fn id(self) -> u64 {
        self.0
    }
}

/// Ordered project edit command batch.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProjectEditBatch {
    pub commands: Vec<ProjectEditCommand>,
}

impl ProjectEditBatch {
    pub fn new(commands: Vec<ProjectEditCommand>) -> Self {
        Self { commands }
    }
}

/// One project edit command with client correlation id.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProjectEditCommand {
    pub id: ProjectEditCommandId,
    pub op: ProjectEditOp,
}

/// Client-facing project edit operation.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum ProjectEditOp {
    ApplyArtifactEdit { edit: ArtifactEdit },
    RemovePendingArtifact { artifact_path: LpPathBuf },
    DiscardOverlay,
    Commit,
}

/// Ordered result for a [`ProjectEditBatch`].
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProjectEditBatchResult {
    pub results: Vec<ProjectEditCommandResult>,
}

impl ProjectEditBatchResult {
    pub fn new(results: Vec<ProjectEditCommandResult>) -> Self {
        Self { results }
    }
}

/// Result for one project edit command.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProjectEditCommandResult {
    pub id: ProjectEditCommandId,
    pub status: ProjectEditCommandStatus,
}

impl ProjectEditCommandResult {
    pub fn accepted(id: ProjectEditCommandId, effect: ProjectEditEffect) -> Self {
        Self {
            id,
            status: ProjectEditCommandStatus::Accepted { effect },
        }
    }

    pub fn rejected(id: ProjectEditCommandId, rejection: ProjectEditRejection) -> Self {
        Self {
            id,
            status: ProjectEditCommandStatus::Rejected { rejection },
        }
    }
}

/// Accepted or rejected command status.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum ProjectEditCommandStatus {
    Accepted { effect: ProjectEditEffect },
    Rejected { rejection: ProjectEditRejection },
}

/// Observable effect of an accepted project edit command.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "effect")]
pub enum ProjectEditEffect {
    PendingChanged { changed: bool },
    Committed { summary: ProjectCommitSummary },
}

/// Portable rejection for a project edit command.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectEditRejection {
    pub reason: ProjectEditRejectionReason,
    pub message: String,
}

impl ProjectEditRejection {
    pub fn new(reason: ProjectEditRejectionReason, message: String) -> Self {
        Self { reason, message }
    }
}

/// Stable reason for a rejected project edit command.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectEditRejectionReason {
    InvalidPath,
    EditFailed,
    CommitFailed,
    Unsupported,
}

/// Portable commit summary.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectCommitSummary {
    pub def_updates: ProjectDefUpdates,
    pub change_details: Vec<(DefinitionLocation, ProjectDefChangeDetail)>,
}

impl ProjectCommitSummary {
    pub fn is_empty(&self) -> bool {
        self.def_updates.is_empty() && self.change_details.is_empty()
    }
}

/// Added, changed, and removed definition locations.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectDefUpdates {
    pub added: Vec<DefinitionLocation>,
    pub changed: Vec<DefinitionLocation>,
    pub removed: Vec<DefinitionLocation>,
}

impl ProjectDefUpdates {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }
}

/// Portable factual definition change classification.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectDefChangeDetail {
    Content,
    KindChanged { from: NodeKind, to: NodeKind },
    EnteredError,
    LeftError,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{ArtifactBodyEdit, ArtifactEdit};

    #[test]
    fn project_edit_batch_round_trips() {
        let batch = ProjectEditBatch::new(alloc::vec![ProjectEditCommand {
            id: ProjectEditCommandId::new(7),
            op: ProjectEditOp::ApplyArtifactEdit {
                edit: ArtifactEdit::body(
                    LpPathBuf::from("/shader.glsl"),
                    ArtifactBodyEdit::ReplaceBody(b"void main() {}".to_vec()),
                ),
            },
        }]);

        let json = serde_json::to_string(&batch).unwrap();
        let decoded: ProjectEditBatch = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, batch);
    }
}
