//! The `lp.node` envelope: one node plus its assets.
//!
//! This is [`lpc_wire::WireCreateNodeRequest`] minus its attach site. That
//! is not a coincidence — the create request was built to carry bytes
//! precisely so that "future sources (copy, import, examples) reuse it
//! unchanged", so copy is the source it was waiting for and paste is a
//! plain `CreateNode` once the target picks a site.
//!
//! Assets travel with the node because a shader without its `.glsl` is not
//! a shader. There is no cloud provider yet, so this envelope is the only
//! way to hand someone a shader you wrote.

use std::collections::BTreeMap;

use lpc_model::{LpPathBuf, NodeAttachSite};
use lpc_wire::WireCreateNodeRequest;
use serde::{Deserialize, Serialize};

use super::share_envelope::{
    NODE_KIND, SHARE_FORMAT_VERSION, check_format, check_kind, peek_header_lenient,
};
use super::share_error::ShareError;
use super::share_file::ShareFile;

/// One node and its sibling assets, ready for the clipboard.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeEnvelope {
    pub kind: String,
    pub format: u32,
    /// The node's display label at the time of copy — paste targets show
    /// it before committing, and it seeds the pasted node's name.
    pub label: String,
    /// Project-relative def file path as it was in the SOURCE project,
    /// e.g. `"./orbit.json"`. The paste site re-derives a free path; this
    /// is a starting point, not a claim on the target.
    pub file: String,
    /// The node def JSON.
    pub body: ShareFile,
    /// Sibling asset files, source-relative path → contents.
    pub assets: BTreeMap<String, ShareFile>,
}

impl NodeEnvelope {
    /// Wrap a node's def bytes and its assets.
    pub fn encode(label: &str, file: &str, body: &[u8], assets: &[(String, Vec<u8>)]) -> Self {
        Self {
            kind: NODE_KIND.to_string(),
            format: SHARE_FORMAT_VERSION,
            label: label.to_string(),
            file: file.to_string(),
            body: ShareFile::from_bytes(body),
            assets: assets
                .iter()
                .map(|(path, bytes)| (path.clone(), ShareFile::from_bytes(bytes)))
                .collect(),
        }
    }

    /// Parse an envelope from pasted text.
    ///
    /// The header is validated **before** the body is deserialized, so
    /// pasting a package envelope here reports "that is an lp.package"
    /// rather than a structural complaint about its fields.
    pub fn decode(text: &str) -> Result<Self, ShareError> {
        let header = peek_header_lenient(text)?;
        check_kind(&header.kind, &[NODE_KIND])?;
        check_format(header.format)?;

        serde_json::from_str(text).map_err(|error| ShareError::Malformed(error.to_string()))
    }

    /// Serialize for the clipboard.
    pub fn to_json(&self) -> Result<String, ShareError> {
        serde_json::to_string_pretty(self).map_err(|error| ShareError::Malformed(error.to_string()))
    }

    /// Build the create request that lands this node at `attach`.
    ///
    /// `file` and `asset_paths` come from the caller because the SOURCE
    /// paths may collide in the target project; the caller resolves free
    /// paths (and rewrites the body's asset references to match) before
    /// calling. `asset_paths` maps each source path to its target path;
    /// an absent entry keeps the source path.
    pub fn to_create_request(
        &self,
        file: &str,
        asset_paths: &BTreeMap<String, String>,
        attach: NodeAttachSite,
    ) -> Result<WireCreateNodeRequest, ShareError> {
        let body = self.body.to_bytes(&self.file)?;
        let assets = self
            .assets
            .iter()
            .map(|(source_path, asset)| {
                let target = asset_paths.get(source_path).unwrap_or(source_path);
                let bytes = asset.to_bytes(source_path)?;
                Ok((LpPathBuf::from(target.as_str()), bytes))
            })
            .collect::<Result<Vec<_>, ShareError>>()?;
        Ok(WireCreateNodeRequest::new(
            LpPathBuf::from(file),
            body,
            assets,
            attach,
        ))
    }

    /// The def body as text, for callers that must rewrite asset
    /// references before pasting. `None` when the body is not UTF-8 (a node
    /// def always is, so this is a corruption signal).
    pub fn body_text(&self) -> Option<&str> {
        match &self.body {
            ShareFile::Text { text } => Some(text),
            ShareFile::Base64 { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::SlotPath;

    use super::*;

    fn envelope() -> NodeEnvelope {
        NodeEnvelope::encode(
            "Orbit",
            "./orbit.json",
            br#"{"kind":"Shader","source":"./orbit.glsl"}"#,
            &[("./orbit.glsl".to_string(), b"void main() {}".to_vec())],
        )
    }

    #[test]
    fn a_node_round_trips_with_its_assets() {
        let json = envelope().to_json().unwrap();
        let decoded = NodeEnvelope::decode(&json).unwrap();
        assert_eq!(decoded, envelope());
        assert_eq!(decoded.label, "Orbit");
        assert_eq!(decoded.assets.len(), 1);
    }

    #[test]
    fn conversion_to_a_create_request_preserves_bytes() {
        let request = envelope()
            .to_create_request(
                "./orbit.json",
                &BTreeMap::new(),
                NodeAttachSite::ProjectNodes {
                    key: "orbit".to_string(),
                },
            )
            .unwrap();

        // `LpPathBuf` normalizes the authored `./` prefix away, so the
        // request carries the same path the loader would resolve.
        assert_eq!(request.file.as_str(), "orbit.json");
        assert_eq!(
            request.body,
            br#"{"kind":"Shader","source":"./orbit.glsl"}"#
        );
        assert_eq!(request.assets.len(), 1);
        assert_eq!(request.assets[0].0.as_str(), "orbit.glsl");
        assert_eq!(request.assets[0].1, b"void main() {}");
    }

    #[test]
    fn a_collision_rename_redirects_both_the_def_and_its_assets() {
        // The target already has an `orbit`, so the paste lands as
        // `orbit-2` and the asset must follow it.
        let mut renames = BTreeMap::new();
        renames.insert("./orbit.glsl".to_string(), "./orbit-2.glsl".to_string());

        let request = envelope()
            .to_create_request(
                "./orbit-2.json",
                &renames,
                NodeAttachSite::ProjectNodes {
                    key: "orbit-2".to_string(),
                },
            )
            .unwrap();

        assert_eq!(request.file.as_str(), "orbit-2.json");
        assert_eq!(request.assets[0].0.as_str(), "orbit-2.glsl");
        assert_eq!(
            request.assets[0].1, b"void main() {}",
            "renaming must not disturb the bytes"
        );
    }

    #[test]
    fn both_attach_sites_are_expressible() {
        let slot = NodeAttachSite::Slot {
            artifact: lpc_model::ArtifactLocation::file("/playlist.json"),
            path: SlotPath::parse("entries[2].node").unwrap(),
        };
        let request = envelope()
            .to_create_request("./orbit.json", &BTreeMap::new(), slot.clone())
            .unwrap();
        assert_eq!(request.attach, slot);
    }

    #[test]
    fn a_package_envelope_is_refused_by_the_node_decoder() {
        let json = r#"{"kind":"lp.package","format":1,"label":"x","file":"./a.json","body":{"text":"{}"},"assets":{}}"#;
        assert_eq!(
            NodeEnvelope::decode(json).unwrap_err(),
            ShareError::UnknownKind {
                kind: "lp.package".to_string()
            }
        );
    }

    #[test]
    fn a_future_format_is_rejected_rather_than_migrated() {
        let json = r#"{"kind":"lp.node","format":7,"label":"x","file":"./a.json","body":{"text":"{}"},"assets":{}}"#;
        assert_eq!(
            NodeEnvelope::decode(json).unwrap_err(),
            ShareError::UnsupportedFormat {
                found: 7,
                supported: 1
            }
        );
    }

    #[test]
    fn a_node_def_body_is_readable_text_for_reference_rewriting() {
        assert!(
            envelope()
                .body_text()
                .expect("a def body is always UTF-8")
                .contains("./orbit.glsl")
        );
    }

    #[test]
    fn an_asset_free_node_needs_no_assets_map() {
        let clock = NodeEnvelope::encode("Clock", "./clock.json", br#"{"kind":"Clock"}"#, &[]);
        let json = clock.to_json().unwrap();
        let request = NodeEnvelope::decode(&json)
            .unwrap()
            .to_create_request(
                "./clock.json",
                &BTreeMap::new(),
                NodeAttachSite::ProjectNodes {
                    key: "clock".to_string(),
                },
            )
            .unwrap();
        assert!(request.assets.is_empty());
    }
}
