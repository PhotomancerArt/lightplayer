//! Project overlay commit envelopes.

use lpc_model::CommitResult;

/// Wire request to commit the current project overlay.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WireOverlayCommitRequest;

/// Wire response containing the artifact writes performed by commit.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WireOverlayCommitResponse {
    pub result: CommitResult,
}

impl WireOverlayCommitResponse {
    pub fn new(result: CommitResult) -> Self {
        Self { result }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_commit_response_round_trips() {
        let response = WireOverlayCommitResponse::new(CommitResult::default());

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireOverlayCommitResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
        assert!(json.contains("artifact_changes"));
    }
}
