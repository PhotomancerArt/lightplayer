//! Project overlay read envelopes.

use lpc_model::{ProjectOverlay, Revision};

/// Wire request for the full pending project overlay.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WireOverlayReadRequest;

/// Wire response containing the full pending project overlay.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WireOverlayReadResponse {
    pub overlay: ProjectOverlay,
    /// Revision at which the overlay last changed (its `changed_at` at read
    /// time), so the client can stamp its mirror.
    pub revision: Revision,
}

impl WireOverlayReadResponse {
    pub fn new(overlay: ProjectOverlay, revision: Revision) -> Self {
        Self { overlay, revision }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::{ArtifactLocation, SlotEdit, SlotPath};

    #[test]
    fn overlay_read_response_round_trips() {
        let mut overlay = ProjectOverlay::new();
        overlay.put_slot_edit(
            ArtifactLocation::file("/project.toml"),
            SlotEdit::ensure_present(SlotPath::parse("nodes[clock]").unwrap()),
        );
        let response = WireOverlayReadResponse::new(overlay, Revision::new(7));

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireOverlayReadResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
        assert_eq!(decoded.revision, Revision::new(7));
        assert!(json.contains("/project.toml"));
        assert!(json.contains("revision"));
    }
}
