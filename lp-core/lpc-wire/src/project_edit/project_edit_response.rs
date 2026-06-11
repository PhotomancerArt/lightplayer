//! Server response envelope for authored project edits.

use lpc_model::ProjectEditBatchResult;

/// Wire envelope for one project edit batch result.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WireProjectEditResponse {
    pub result: ProjectEditBatchResult,
}

impl WireProjectEditResponse {
    pub fn new(result: ProjectEditBatchResult) -> Self {
        Self { result }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use alloc::vec;
    use lpc_model::{
        ProjectEditBatchResult, ProjectEditCommandId, ProjectEditCommandResult, ProjectEditEffect,
        ProjectEditRejection, ProjectEditRejectionReason,
    };

    #[test]
    fn project_edit_response_round_trips() {
        let response = WireProjectEditResponse::new(ProjectEditBatchResult::new(vec![
            ProjectEditCommandResult::accepted(
                ProjectEditCommandId::new(1),
                ProjectEditEffect::PendingChanged { changed: true },
            ),
            ProjectEditCommandResult::rejected(
                ProjectEditCommandId::new(2),
                ProjectEditRejection::new(
                    ProjectEditRejectionReason::InvalidPath,
                    String::from("path must be absolute"),
                ),
            ),
        ]));

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireProjectEditResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
        assert!(json.contains("pending_changed"));
        assert!(json.contains("invalid_path"));
    }
}
