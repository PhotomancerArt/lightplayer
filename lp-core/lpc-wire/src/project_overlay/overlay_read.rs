//! Project overlay read envelopes.

use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::{ArtifactLocation, ProjectOverlay, Revision, SlotPath};

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
    /// Base (saved) value display strings for the overlay's pending slot-edit
    /// paths, as a lean parallel list beside the overlay (the overlay itself
    /// carries no annotations). Paths whose base target is absent are
    /// omitted; the client renders them as "not set". Omitted from the wire
    /// entirely when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub base_values: Vec<(ArtifactLocation, SlotPath, String)>,
}

impl WireOverlayReadResponse {
    pub fn new(overlay: ProjectOverlay, revision: Revision) -> Self {
        Self {
            overlay,
            revision,
            base_values: Vec::new(),
        }
    }

    /// Attach the base-value display annotations for the overlay's paths.
    pub fn with_base_values(
        mut self,
        base_values: Vec<(ArtifactLocation, SlotPath, String)>,
    ) -> Self {
        self.base_values = base_values;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;
    use lpc_model::{SlotEdit, SlotPath};

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
        assert!(
            !json.contains("base_values"),
            "empty base values stay off the wire: {json}"
        );
    }

    #[test]
    fn overlay_read_response_round_trips_base_values() {
        let mut overlay = ProjectOverlay::new();
        let artifact = ArtifactLocation::file("/clock.json");
        let path = SlotPath::parse("controls.rate").unwrap();
        overlay.put_slot_edit(
            artifact.clone(),
            SlotEdit::assign_value(path.clone(), lpc_model::LpValue::F32(2.0)),
        );
        let response = WireOverlayReadResponse::new(overlay, Revision::new(9))
            .with_base_values(vec![(artifact.clone(), path.clone(), "1.0".to_string())]);

        let json = serde_json::to_string(&response).unwrap();
        let decoded: WireOverlayReadResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, response);
        assert_eq!(
            decoded.base_values,
            vec![(artifact, path, "1.0".to_string())]
        );
        assert!(json.contains("base_values"), "{json}");
    }
}
