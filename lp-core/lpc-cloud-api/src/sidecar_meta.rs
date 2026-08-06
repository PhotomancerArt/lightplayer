//! Client-computed display metadata carried alongside a commit.

use alloc::string::String;
use lpc_history::ContentHash;
use serde::{Deserialize, Serialize};

/// Display metadata for a project, computed client-side and pushed with
/// every commit (D3) rather than derived server-side from the tree — the
/// server never opens project content to render a listing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarMeta {
    /// Display name shown in project lists.
    pub name: String,
    /// The project package format version the commit was authored at (see
    /// the schema/shape format-version gate in `lpc-model`) — lets the
    /// server refuse or flag content it cannot yet render a preview for
    /// without opening the tree.
    pub format_version: u32,
    /// Content hash of a client-rendered PNG preview, if one was generated.
    /// The blob itself travels the separate blob-plane HTTP transfer; only
    /// its hash lives in the vocabulary.
    pub preview_png: Option<ContentHash>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    fn sample() -> SidecarMeta {
        SidecarMeta {
            name: "Zook Dome".to_string(),
            format_version: 4,
            preview_png: Some(ContentHash::of(b"preview-bytes")),
        }
    }

    #[test]
    fn serde_round_trip() {
        let meta = sample();
        let json = serde_json::to_string(&meta).unwrap();
        let back: SidecarMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(back, meta);
    }

    #[test]
    fn serde_round_trip_no_preview() {
        let meta = SidecarMeta {
            name: "Untitled".to_string(),
            format_version: 1,
            preview_png: None,
        };
        let json = serde_json::to_string(&meta).unwrap();
        let back: SidecarMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(back, meta);
    }

    /// Pinned JSON literal: the deployed format is the contract.
    #[test]
    fn pinned_json_literal() {
        let meta = SidecarMeta {
            name: "Zook Dome".to_string(),
            format_version: 4,
            preview_png: None,
        };
        assert_eq!(
            serde_json::to_string(&meta).unwrap(),
            r#"{"name":"Zook Dome","formatVersion":4,"previewPng":null}"#
        );
    }
}
