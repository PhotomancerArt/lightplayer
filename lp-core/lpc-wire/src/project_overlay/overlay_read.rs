//! Project overlay read envelopes.

use lpc_model::ProjectOverlay;

/// Wire request for the full pending project overlay.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WireOverlayReadRequest;

/// Wire response containing the full pending project overlay.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WireOverlayReadResponse {
    pub overlay: ProjectOverlay,
}

impl WireOverlayReadResponse {
    pub fn new(overlay: ProjectOverlay) -> Self {
        Self { overlay }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::{LpPathBuf, SlotEdit, SlotPath};

    #[test]
    fn overlay_read_response_round_trips() {
        let mut overlay = ProjectOverlay::new();
        overlay.put_slot_edit(
            LpPathBuf::from("/project.toml"),
            SlotEdit::ensure_present(SlotPath::parse("nodes[clock]").unwrap()),
        );
        let response = WireOverlayReadResponse::new(overlay);

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireOverlayReadResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
        assert!(json.contains("/project.toml"));
    }
}
