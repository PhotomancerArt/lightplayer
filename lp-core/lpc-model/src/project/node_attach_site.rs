//! Attachment-site vocabulary for dedicated node authoring operations.
//!
//! A [`NodeAttachSite`] names where a newly created node hooks into the
//! project structure. It is shared domain vocabulary: the `CreateNode` wire
//! command carries it, and the registry-side create/remove operations resolve
//! it against the effective project.

use alloc::string::String;

use crate::{ArtifactLocation, SlotPath};

/// Where a node attaches in the project structure.
///
/// Two site families exist today:
///
/// - the project root's `nodes` map, whose role is `Fixed` (read-only) for
///   generic slot edits and reachable **only** through dedicated node
///   operations, and
/// - any writable [`crate::NodeInvocationSlot`]-shaped slot in another
///   definition (e.g. a playlist's `entries[k].node`), addressed by the
///   artifact holding the slot plus the slot path inside it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeAttachSite {
    /// `project.json` `nodes[key]` — the policy-locked site owned by
    /// dedicated node operations.
    ProjectNodes { key: String },
    /// Any writable `NodeInvocationSlot`, e.g. a playlist's
    /// `entries[k].node`. `artifact` is the definition file holding the
    /// slot; `path` addresses the invocation slot inside it.
    Slot {
        artifact: ArtifactLocation,
        path: SlotPath,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn node_attach_site_round_trips_both_forms() {
        let project = NodeAttachSite::ProjectNodes {
            key: "shader-2".to_string(),
        };
        let json = serde_json::to_string(&project).unwrap();
        let decoded: NodeAttachSite = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, project);
        assert!(json.contains("project_nodes"));

        let slot = NodeAttachSite::Slot {
            artifact: ArtifactLocation::file("/playlist.json"),
            path: SlotPath::parse("entries[2].node").unwrap(),
        };
        let json = serde_json::to_string(&slot).unwrap();
        let decoded: NodeAttachSite = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, slot);
        assert!(json.contains("entries[2].node"));
    }
}
