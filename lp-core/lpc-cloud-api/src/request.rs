//! Everything a client can ask the cloud service for.
//!
//! Each request is a **struct**, and [`CloudRequest`] is the closed set of
//! them: a unit variant for the two requests that carry no payload, and a
//! newtype variant wrapping the struct for the rest. The struct is the source
//! of truth for what a request carries, which is what lets
//! [`CloudCallSpec`](crate::call_spec::CloudCallSpec) name the one response
//! shape each one can produce.
//!
//! # Why the wire form did not move
//!
//! Serde's external tagging writes a newtype variant as
//! `{"getProject": <the inner struct>}` — exactly what the struct variant
//! `GetProject { uid }` wrote before. The enum's `rename_all = "camelCase"`
//! renames *variants*, never fields, so the structs below deliberately carry
//! no `rename_all` of their own: their field names are already the deployed
//! ones. The pinned literal tests at the bottom of this file are the check.

use alloc::string::String;
use alloc::vec::Vec;
use lpc_history::{ContentHash, HistoryEvent, PrefixedUid};
use serde::{Deserialize, Serialize};

use crate::sidecar_meta::SidecarMeta;
use crate::visibility::Visibility;

/// A client→service request. See [`crate::response::CloudResponse`] for the
/// matching typed answers, [`crate::call_spec::CloudCallSpec`] for the
/// request→response pairing, and [`crate::envelope::CloudCall`] for the
/// version-carrying envelope this travels in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloudRequest {
    /// See [`WhoAmI`].
    WhoAmI,
    /// See [`ListMyProjects`].
    ListMyProjects,
    /// See [`PublishProject`].
    PublishProject(PublishProject),
    /// See [`SetVisibility`].
    SetVisibility(SetVisibility),
    /// See [`AddMember`].
    AddMember(AddMember),
    /// See [`RemoveMember`].
    RemoveMember(RemoveMember),
    /// See [`GetProject`].
    GetProject(GetProject),
    /// See [`GetHeads`].
    GetHeads(GetHeads),
    /// See [`HaveBlobs`].
    HaveBlobs(HaveBlobs),
    /// See [`PushCommit`].
    PushCommit(PushCommit),
    /// See [`GetEvents`].
    GetEvents(GetEvents),
}

/// Who is the caller? Answered with [`crate::response::UserInfo`]; never
/// fails on an anonymous caller — it just reports `Actor::Anonymous`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhoAmI;

/// List every project the caller owns or is a member of. Answered with
/// [`crate::response::ProjectList`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListMyProjects;

/// Publish a project to the cloud service for the first time, minting its
/// server-side [`crate::project_meta::ProjectMeta`] at the given `slug` and
/// `visibility`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublishProject {
    /// The project's uid (already minted client-side).
    pub uid: PrefixedUid,
    /// Initial access level.
    pub visibility: Visibility,
    /// Human-readable slug for share URLs.
    pub slug: String,
}

/// Change an already-published project's access level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetVisibility {
    /// The project to change.
    pub uid: PrefixedUid,
    /// The new access level.
    pub visibility: Visibility,
}

/// Grant a user access to a `Private` project by email.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddMember {
    /// The project to grant access to.
    pub uid: PrefixedUid,
    /// The email identifying the account to add.
    pub email: String,
}

/// Revoke a user's access to a project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoveMember {
    /// The project to revoke access to.
    pub uid: PrefixedUid,
    /// The email identifying the account to remove.
    pub email: String,
}

/// Fetch a project's current metadata, heads, and sidecar. Anonymous callers
/// are answered when the project is `Visibility::Link`; a `Private` project
/// answers [`crate::error::CloudError::NotFound`] for anyone without access,
/// including anonymous callers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetProject {
    /// The project to fetch.
    pub uid: PrefixedUid,
}

/// Fetch a project's current head set. Answered with
/// [`crate::response::Heads`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetHeads {
    /// The project whose heads to fetch.
    pub uid: PrefixedUid,
}

/// Ask which of these content hashes the server does not already have,
/// before spending a blob-plane HTTP upload on hashes it already holds.
/// Answered with [`crate::response::MissingBlobs`] — the subset the server is
/// missing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HaveBlobs {
    /// Candidate hashes to check.
    pub hashes: Vec<ContentHash>,
}

/// Push a new commit onto a project's history. Never rejected for diverging
/// from the current head — see [`crate::head_info::PushOutcome`] — only for
/// missing blobs or a version/auth failure. Answered with
/// [`crate::response::PushResult`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PushCommit {
    /// The project being pushed to.
    pub uid: PrefixedUid,
    /// Parent tree hashes this commit builds on.
    pub parents: Vec<ContentHash>,
    /// The pushed commit's own tree hash.
    pub tree: ContentHash,
    /// The history events this commit adds.
    pub events: Vec<HistoryEvent>,
    /// Client-computed display metadata for the commit.
    pub sidecar: SidecarMeta,
}

/// Fetch server event-log entries for a project since a given server event
/// sequence number (not a `HistoryEvent` timestamp). Answered with
/// [`crate::response::Events`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetEvents {
    /// The project whose event log to read.
    pub uid: PrefixedUid,
    /// Server event sequence number to read forward from (exclusive).
    pub since: u64,
}

impl From<WhoAmI> for CloudRequest {
    fn from(_: WhoAmI) -> Self {
        CloudRequest::WhoAmI
    }
}

impl From<ListMyProjects> for CloudRequest {
    fn from(_: ListMyProjects) -> Self {
        CloudRequest::ListMyProjects
    }
}

impl From<PublishProject> for CloudRequest {
    fn from(request: PublishProject) -> Self {
        CloudRequest::PublishProject(request)
    }
}

impl From<SetVisibility> for CloudRequest {
    fn from(request: SetVisibility) -> Self {
        CloudRequest::SetVisibility(request)
    }
}

impl From<AddMember> for CloudRequest {
    fn from(request: AddMember) -> Self {
        CloudRequest::AddMember(request)
    }
}

impl From<RemoveMember> for CloudRequest {
    fn from(request: RemoveMember) -> Self {
        CloudRequest::RemoveMember(request)
    }
}

impl From<GetProject> for CloudRequest {
    fn from(request: GetProject) -> Self {
        CloudRequest::GetProject(request)
    }
}

impl From<GetHeads> for CloudRequest {
    fn from(request: GetHeads) -> Self {
        CloudRequest::GetHeads(request)
    }
}

impl From<HaveBlobs> for CloudRequest {
    fn from(request: HaveBlobs) -> Self {
        CloudRequest::HaveBlobs(request)
    }
}

impl From<PushCommit> for CloudRequest {
    fn from(request: PushCommit) -> Self {
        CloudRequest::PushCommit(request)
    }
}

impl From<GetEvents> for CloudRequest {
    fn from(request: GetEvents) -> Self {
        CloudRequest::GetEvents(request)
    }
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
        let req = CloudRequest::PublishProject(PublishProject {
            uid: uid(),
            visibility: Visibility::Link,
            slug: "zook-dome".to_string(),
        });
        let json = serde_json::to_string(&req).unwrap();
        let back: CloudRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn serde_round_trip_push_commit() {
        let req = CloudRequest::PushCommit(PushCommit {
            uid: uid(),
            parents: vec![ContentHash::of(b"parent")],
            tree: ContentHash::of(b"tree"),
            events: vec![],
            sidecar: SidecarMeta {
                name: "Zook Dome".to_string(),
                format_version: 4,
                preview_png: None,
            },
        });
        let json = serde_json::to_string(&req).unwrap();
        let back: CloudRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn serde_round_trip_get_events() {
        let req = CloudRequest::GetEvents(GetEvents {
            uid: uid(),
            since: 42,
        });
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
        let req = CloudRequest::GetProject(GetProject {
            uid: PrefixedUid::mint(UidPrefix::Project, &[0u8; 16]),
        });
        assert_eq!(
            serde_json::to_string(&req).unwrap(),
            r#"{"getProject":{"uid":"prj_0000000000000000"}}"#
        );
    }

    /// The multi-field family pinned too: newtype wrapping must not have
    /// nested the payload one level deeper.
    #[test]
    fn pinned_json_literal_get_events() {
        let req = CloudRequest::GetEvents(GetEvents {
            uid: PrefixedUid::mint(UidPrefix::Project, &[0u8; 16]),
            since: 7,
        });
        assert_eq!(
            serde_json::to_string(&req).unwrap(),
            r#"{"getEvents":{"uid":"prj_0000000000000000","since":7}}"#
        );
    }
}
