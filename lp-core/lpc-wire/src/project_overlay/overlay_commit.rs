//! Project overlay commit envelopes.

use lpc_model::{CommitResult, Revision};

/// Wire request to commit the current project overlay.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WireOverlayCommitRequest;

/// Wire response containing the artifact writes performed by commit.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WireOverlayCommitResponse {
    pub result: CommitResult,
    /// Revision at which the overlay last changed, after the commit. The
    /// client re-syncs its overlay mirror from this instead of guessing.
    pub overlay_revision: Revision,
}

impl WireOverlayCommitResponse {
    pub fn new(result: CommitResult, overlay_revision: Revision) -> Self {
        Self {
            result,
            overlay_revision,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_commit_response_round_trips() {
        let response = WireOverlayCommitResponse::new(CommitResult::default(), Revision::new(13));

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireOverlayCommitResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
        assert_eq!(decoded.overlay_revision, Revision::new(13));
        assert!(json.contains("artifact_changes"));
        assert!(json.contains("overlay_revision"));
    }
}
