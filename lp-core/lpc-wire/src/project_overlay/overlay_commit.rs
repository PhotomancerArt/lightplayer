//! Project overlay commit envelopes.

use lpc_model::ProjectCommitSummary;

/// Wire request to commit the current project overlay.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WireOverlayCommitRequest;

/// Wire response containing the portable commit summary.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WireOverlayCommitResponse {
    pub summary: ProjectCommitSummary,
}

impl WireOverlayCommitResponse {
    pub fn new(summary: ProjectCommitSummary) -> Self {
        Self { summary }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_commit_response_round_trips() {
        let response = WireOverlayCommitResponse::new(ProjectCommitSummary::default());

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireOverlayCommitResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
        assert!(json.contains("def_updates"));
    }
}
