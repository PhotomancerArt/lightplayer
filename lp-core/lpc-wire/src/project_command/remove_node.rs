//! Node removal envelopes.
//!
//! `RemoveNode` atomically **stages** a node removal in the project overlay
//! (unlike `CreateNode`, which commits immediately): a `Remove` slot edit at
//! the attachment site, `Delete` body overlays on the removed subtree's def
//! and exclusively referenced assets, and a sweep of any pending overlay
//! intent belonging to the removed subtree. The removal stays revertible with
//! existing overlay mutations (`RemoveSlotEdit` at the site + `ClearArtifact`
//! per staged delete) until the overlay is committed.

use alloc::vec::Vec;

use lpc_model::{ArtifactLocation, MutationRejection, NodeAttachSite, Revision};

/// Wire request to stage the removal of one attached node.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WireRemoveNodeRequest {
    /// Attachment site of the node to remove. The site must resolve in the
    /// effective inventory; for map-entry sites (project `nodes[key]`,
    /// playlist `entries[k]`) the whole entry is removed.
    pub site: NodeAttachSite,
}

impl WireRemoveNodeRequest {
    pub fn new(site: NodeAttachSite) -> Self {
        Self { site }
    }
}

/// Wire response for a node removal.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireRemoveNodeResponse {
    /// Removal staged in the overlay: the node left the effective tree and
    /// the runtime, and the listed artifacts delete on the next commit.
    Staged {
        /// Revision at which the overlay last changed, after staging.
        overlay_revision: Revision,
        /// Artifacts staged `AssetBodyOverlay::Delete`: the removed node's
        /// def plus everything exclusively referenced by the removed subtree.
        staged_deletes: Vec<ArtifactLocation>,
        /// Whether staging swept pending overlay edits (edits under the
        /// removed entry, or pending intent on removed-subtree artifacts).
        /// Swept edits are not restored by reverting the removal.
        swept_pending_edits: bool,
    },
    /// Removal rejected before any staging; nothing changed.
    Rejected { rejection: MutationRejection },
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::{MutationRejectionReason, SlotPath};

    #[test]
    fn remove_node_request_round_trips_both_sites() {
        let request = WireRemoveNodeRequest::new(NodeAttachSite::ProjectNodes {
            key: "shader-2".into(),
        });
        let json = serde_json::to_string(&request).unwrap();
        let decoded: WireRemoveNodeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, request);
        assert!(json.contains("project_nodes"));

        let request = WireRemoveNodeRequest::new(NodeAttachSite::Slot {
            artifact: ArtifactLocation::file("/playlist.json"),
            path: SlotPath::parse("entries[2].node").unwrap(),
        });
        let json = serde_json::to_string(&request).unwrap();
        let decoded: WireRemoveNodeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, request);
        assert!(json.contains("entries[2].node"));
    }

    #[test]
    fn remove_node_response_round_trips_staged_and_rejected() {
        let staged = WireRemoveNodeResponse::Staged {
            overlay_revision: Revision::new(9),
            staged_deletes: alloc::vec![
                ArtifactLocation::file("/shader-2.json"),
                ArtifactLocation::file("/shader-2.glsl"),
            ],
            swept_pending_edits: true,
        };
        let json = serde_json::to_string(&staged).unwrap();
        let decoded: WireRemoveNodeResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, staged);
        assert!(json.contains("staged_deletes"));
        assert!(json.contains("swept_pending_edits"));

        let rejected = WireRemoveNodeResponse::Rejected {
            rejection: MutationRejection::new(
                MutationRejectionReason::UnknownSlotPath,
                "project node key shader-2 does not exist".into(),
            ),
        };
        let json = serde_json::to_string(&rejected).unwrap();
        let decoded: WireRemoveNodeResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, rejected);
        assert!(json.contains("unknown_slot_path"));
    }
}
