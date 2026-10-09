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
    /// Node def JSON bytes (canonical `write_json` output). On the wire,
    /// text as itself, other bytes as `{"base64":"…"}`
    /// ([`lpc_model::body_bytes`]).
    #[serde(with = "lpc_model::body_bytes")]
    pub body: Vec<u8>,
    /// Sibling asset files to create, e.g. `[("./shader-2.glsl", …)]`, each
    /// body encoded as [`Self::body`] is.
    #[serde(with = "asset_bodies")]
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

/// `(path, body)` pairs with each body as [`lpc_model::body_bytes`] writes
/// it.
mod asset_bodies {
    use alloc::vec::Vec;

    use lpc_model::LpPathBuf;
    use lpc_model::body_bytes::{BodyBuf, BodyRef};
    use serde::ser::SerializeSeq;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        assets: &[(LpPathBuf, Vec<u8>)],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(assets.len()))?;
        for (path, bytes) in assets {
            seq.serialize_element(&(path, BodyRef(bytes)))?;
        }
        seq.end()
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<(LpPathBuf, Vec<u8>)>, D::Error> {
        let assets = Vec::<(LpPathBuf, BodyBuf)>::deserialize(deserializer)?;
        Ok(assets
            .into_iter()
            .map(|(path, BodyBuf(bytes))| (path, bytes))
            .collect())
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
        // The def and the shader go as their own text, not as byte arrays.
        assert!(
            json.contains(r#""body":"{\n  \"kind\": \"Shader\"\n}\n""#),
            "{json}"
        );
        assert!(
            json.contains(r#""assets":[["shader-2.glsl","void main() {}"]]"#),
            "{json}"
        );

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
    fn a_binary_asset_goes_as_base64() {
        let request = WireCreateNodeRequest::new(
            LpPathBuf::from("./image.json"),
            b"{}".to_vec(),
            alloc::vec![(
                LpPathBuf::from("./image.png"),
                alloc::vec![0x89, b'P', 0xff]
            )],
            NodeAttachSite::ProjectNodes {
                key: "image".into(),
            },
        );
        let json = serde_json::to_string(&request).unwrap();
        assert!(
            json.contains(r#""assets":[["image.png",{"base64":"iVD/"}]]"#),
            "{json}"
        );
        let decoded: WireCreateNodeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, request);
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
