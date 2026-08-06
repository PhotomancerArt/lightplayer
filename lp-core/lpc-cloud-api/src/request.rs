//! Everything a client can ask the cloud service for.

use alloc::string::String;
use alloc::vec::Vec;
use lpc_history::{ContentHash, HistoryEvent, PrefixedUid};
use serde::{Deserialize, Serialize};

use crate::sidecar_meta::SidecarMeta;
use crate::visibility::Visibility;

/// A client→service request. See [`crate::response::CloudResponse`] for the
/// matching typed answers and [`crate::envelope::CloudCall`] for the
/// version-carrying envelope this travels in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloudRequest {
    /// Who is the caller? Answered with
    /// [`crate::response::CloudResponse::UserInfo`]; never fails on an
    /// anonymous caller — it just reports `Actor::Anonymous`.
    WhoAmI,
    /// List every project the caller owns or is a member of. Answered with
    /// [`crate::response::CloudResponse::ProjectList`].
    ListMyProjects,
    /// Publish a project to the cloud service for the first time, minting
    /// its server-side [`crate::project_meta::ProjectMeta`] at the given
    /// `slug` and `visibility`.
    PublishProject {
        /// The project's uid (already minted client-side).
        uid: PrefixedUid,
        /// Initial access level.
        visibility: Visibility,
        /// Human-readable slug for share URLs.
        slug: String,
    },
    /// Change an already-published project's access level.
    SetVisibility {
        /// The project to change.
        uid: PrefixedUid,
        /// The new access level.
        visibility: Visibility,
    },
    /// Grant a user access to a `Private` project by email.
    AddMember {
        /// The project to grant access to.
        uid: PrefixedUid,
        /// The email identifying the account to add.
        email: String,
    },
    /// Revoke a user's access to a project.
    RemoveMember {
        /// The project to revoke access to.
        uid: PrefixedUid,
        /// The email identifying the account to remove.
        email: String,
    },
    /// Fetch a project's current metadata, heads, and sidecar. Anonymous
    /// callers are answered when the project is `Visibility::Link`; a
    /// `Private` project answers [`crate::error::CloudError::NotFound`] for
    /// anyone without access, including anonymous callers.
    GetProject {
        /// The project to fetch.
        uid: PrefixedUid,
    },
    /// Fetch a project's current head set. Answered with
    /// [`crate::response::CloudResponse::Heads`].
    GetHeads {
        /// The project whose heads to fetch.
        uid: PrefixedUid,
    },
    /// Ask which of these content hashes the server does not already have,
    /// before spending a blob-plane HTTP upload on hashes it already holds.
    /// Answered with [`crate::response::CloudResponse::MissingBlobs`] — the
    /// subset the server is missing.
    HaveBlobs {
        /// Candidate hashes to check.
        hashes: Vec<ContentHash>,
    },
    /// Push a new commit onto a project's history. Never rejected for
    /// diverging from the current head — see
    /// [`crate::head_info::PushOutcome`] — only for missing blobs or a
    /// version/auth failure. Answered with
    /// [`crate::response::CloudResponse::PushResult`].
    PushCommit {
        /// The project being pushed to.
        uid: PrefixedUid,
        /// Parent tree hashes this commit builds on.
        parents: Vec<ContentHash>,
        /// The pushed commit's own tree hash.
        tree: ContentHash,
        /// The history events this commit adds.
        events: Vec<HistoryEvent>,
        /// Client-computed display metadata for the commit.
        sidecar: SidecarMeta,
    },
    /// Fetch server event-log entries for a project since a given server
    /// event sequence number (not a `HistoryEvent` timestamp). Answered
    /// with [`crate::response::CloudResponse::Events`].
    GetEvents {
        /// The project whose event log to read.
        uid: PrefixedUid,
        /// Server event sequence number to read forward from (exclusive).
        since: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;
    use lpc_history::UidPrefix;

    fn uid() -> PrefixedUid {
        PrefixedUid::mint(UidPrefix::Project, &[1u8; 16])
    }

    #[test]
    fn serde_round_trip_unit_variant() {
        let json = serde_json::to_string(&CloudRequest::WhoAmI).unwrap();
        let back: CloudRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, CloudRequest::WhoAmI);
    }

    #[test]
    fn serde_round_trip_publish_project() {
        let req = CloudRequest::PublishProject {
            uid: uid(),
            visibility: Visibility::Link,
            slug: "zook-dome".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: CloudRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn serde_round_trip_push_commit() {
        let req = CloudRequest::PushCommit {
            uid: uid(),
            parents: vec![ContentHash::of(b"parent")],
            tree: ContentHash::of(b"tree"),
            events: vec![],
            sidecar: SidecarMeta {
                name: "Zook Dome".to_string(),
                format_version: 4,
                preview_png: None,
            },
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: CloudRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn serde_round_trip_get_events() {
        let req = CloudRequest::GetEvents {
            uid: uid(),
            since: 42,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: CloudRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, req);
    }

    /// Pinned JSON literals: one representative per message family (unit,
    /// single-field, and multi-field variants). The deployed format is the
    /// contract.
    #[test]
    fn pinned_json_literal_who_am_i() {
        assert_eq!(
            serde_json::to_string(&CloudRequest::WhoAmI).unwrap(),
            "\"whoAmI\""
        );
    }

    #[test]
    fn pinned_json_literal_get_project() {
        let req = CloudRequest::GetProject {
            uid: PrefixedUid::mint(UidPrefix::Project, &[0u8; 16]),
        };
        assert_eq!(
            serde_json::to_string(&req).unwrap(),
            r#"{"getProject":{"uid":"prj_0000000000000000"}}"#
        );
    }
}
