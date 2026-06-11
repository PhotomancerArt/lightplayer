//! Client request envelope for authored project edits.

use lpc_model::ProjectEditBatch;

/// Wire envelope for one project edit batch.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WireProjectEditRequest {
    pub batch: ProjectEditBatch,
}

impl WireProjectEditRequest {
    pub fn new(batch: ProjectEditBatch) -> Self {
        Self { batch }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lpc_model::{
        ArtifactBodyEdit, ArtifactEdit, LpPathBuf, ProjectEditCommand, ProjectEditCommandId,
        ProjectEditOp,
    };

    #[test]
    fn project_edit_request_round_trips() {
        let request = WireProjectEditRequest::new(ProjectEditBatch::new(vec![
            ProjectEditCommand {
                id: ProjectEditCommandId::new(1),
                op: ProjectEditOp::ApplyArtifactEdit {
                    edit: ArtifactEdit::body(
                        LpPathBuf::from("/shader.glsl"),
                        ArtifactBodyEdit::ReplaceBody(b"void main() {}".to_vec()),
                    ),
                },
            },
            ProjectEditCommand {
                id: ProjectEditCommandId::new(2),
                op: ProjectEditOp::Commit,
            },
        ]));

        let json = serde_json::to_string(&request).unwrap();
        let decoded: WireProjectEditRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, request);
        assert!(json.contains("apply_artifact_edit"));
        assert!(json.contains("commit"));
    }
}
