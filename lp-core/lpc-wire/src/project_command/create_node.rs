//! Node creation envelopes.
//!
//! `CreateNode` atomically writes zero or more asset files plus one node-def
//! file, attaches the node at a [`NodeAttachSite`], and live-applies the
//! result to the running engine. The request carries **bytes** so future
//! sources (copy, import, examples) reuse it unchanged. Creation commits
//! immediately to the project filesystem — it never stages in the overlay
//! (`ArtifactOverlay` is slot-XOR-asset, so a staged node body would vanish
//! on reload).

use alloc::vec::Vec;

use lpc_model::{ArtifactChangeSummary, LpPathBuf, MutationRejection, NodeAttachSite, Revision};

/// Wire request to create and attach one node.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WireCreateNodeRequest {
    /// Project-relative def file path, e.g. `"./shader-2.json"`.
    pub file: LpPathBuf,
    /// Node def JSON bytes (canonical `write_json` output).
    pub body: Vec<u8>,
    /// Sibling asset files to create, e.g. `[("./shader-2.glsl", …)]`.
    pub assets: Vec<(LpPathBuf, Vec<u8>)>,
    /// Where the new node attaches.
    pub attach: NodeAttachSite,
}

impl WireCreateNodeRequest {
    pub fn new(
        file: LpPathBuf,
        body: Vec<u8>,
        assets: Vec<(LpPathBuf, Vec<u8>)>,
        attach: NodeAttachSite,
    ) -> Self {
        Self {
            file,
            body,
            assets,
            attach,
        }
    }
}

/// Wire response for a node creation.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireCreateNodeResponse {
    /// Creation applied: files are on disk and the runtime was refreshed.
    Created {
        /// Files written by the operation (created def/assets plus the
        /// rewritten attach artifact).
        artifact_changes: ArtifactChangeSummary,
        /// Revision at which the effective inventory re-derived; gated
        /// project reads from here deliver the new node.
        revision: Revision,
    },
    /// Creation rejected before any write; nothing changed.
    Rejected { rejection: MutationRejection },
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::{ArtifactLocation, MutationRejectionReason, SlotPath};

    #[test]
    fn create_node_request_round_trips_both_attach_sites() {
        let request = WireCreateNodeRequest::new(
            LpPathBuf::from("./shader-2.json"),
            b"{\n  \"kind\": \"Shader\"\n}\n".to_vec(),
            alloc::vec![(
                LpPathBuf::from("./shader-2.glsl"),
                b"void main() {}".to_vec()
            )],
            NodeAttachSite::ProjectNodes {
                key: "shader-2".into(),
            },
        );

        let json = serde_json::to_string(&request).unwrap();
        let decoded: WireCreateNodeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, request);
        assert!(json.contains("project_nodes"));

        let request = WireCreateNodeRequest::new(
            LpPathBuf::from("./visual.json"),
            b"{\n  \"kind\": \"Clock\"\n}\n".to_vec(),
            Vec::new(),
            NodeAttachSite::Slot {
                artifact: ArtifactLocation::file("/playlist.json"),
                path: SlotPath::parse("entries[2].node").unwrap(),
            },
        );

        let json = serde_json::to_string(&request).unwrap();
        let decoded: WireCreateNodeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, request);
        assert!(json.contains("entries[2].node"));
    }

    #[test]
    fn create_node_response_round_trips_created_and_rejected() {
        let created = WireCreateNodeResponse::Created {
            artifact_changes: ArtifactChangeSummary {
                added: alloc::vec![ArtifactLocation::file("/shader-2.json")],
                changed: alloc::vec![ArtifactLocation::file("/module.json")],
                removed: Vec::new(),
            },
            revision: Revision::new(7),
        };
        let json = serde_json::to_string(&created).unwrap();
        let decoded: WireCreateNodeResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, created);
        assert!(json.contains("artifact_changes"));

        let rejected = WireCreateNodeResponse::Rejected {
            rejection: MutationRejection::new(
                MutationRejectionReason::TargetOccupied,
                "node key shader-2 already exists".into(),
            ),
        };
        let json = serde_json::to_string(&rejected).unwrap();
        let decoded: WireCreateNodeResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, rejected);
        assert!(json.contains("target_occupied"));
    }
}
