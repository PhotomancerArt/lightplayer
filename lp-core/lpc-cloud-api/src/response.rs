//! Typed answers to every [`crate::request::CloudRequest`].
//!
//! Responses mirror requests one-to-one where the request produces a
//! distinct payload; a request whose only job is to mutate project state
//! (`PublishProject`, `SetAccess`, `ArchiveProject`, `RestoreProject`,
//! `AddMember`, `RemoveMember`) answers with the resulting [`ProjectInfo`]
//! rather than a bare acknowledgement, so the caller never needs a follow-up
//! `GetProject` to see what it just changed.
//!
//! Like the requests, each response is a **struct** and [`CloudResponse`] is
//! the closed set of them as newtype variants. External tagging means the
//! wire form is unchanged from the struct-variant spelling: `{"heads": <the
//! inner struct>}`. The structs carry no `rename_all` — the enum's applies to
//! variant names only, so `next_since` below is on the wire exactly as
//! spelled.

use alloc::vec::Vec;
use lpc_history::{ContentHash, HistoryEvent};
use serde::{Deserialize, Serialize};

use crate::ack::Ack;
use crate::actor::Actor;
use crate::head_info::{HeadInfo, PushOutcome};
use crate::login_options::LoginOptionsInfo;
use crate::me_info::MeInfo;
use crate::member_info::MemberInfo;
use crate::project_meta::ProjectMeta;
use crate::session_info::SessionList;
use crate::sidecar_meta::SidecarMeta;

/// A service→client response, carried inside a
/// [`crate::envelope::CloudReply`]'s `Ok` side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloudResponse {
    /// See [`UserInfo`].
    UserInfo(UserInfo),
    /// See [`ProjectList`].
    ProjectList(ProjectList),
    /// See [`ProjectInfo`].
    ProjectInfo(ProjectInfo),
    /// See [`Heads`].
    Heads(Heads),
    /// See [`MissingBlobs`].
    MissingBlobs(MissingBlobs),
    /// See [`PushResult`].
    PushResult(PushResult),
    /// See [`Events`].
    Events(Events),
    /// See [`crate::me_info::MeInfo`].
    MeInfo(MeInfo),
    /// See [`crate::session_info::SessionList`].
    SessionList(SessionList),
    /// See [`crate::ack::Ack`].
    Ack(Ack),
    /// See [`crate::login_options::LoginOptionsInfo`].
    LoginOptionsInfo(LoginOptionsInfo),
}

/// Answers [`crate::request::WhoAmI`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserInfo {
    /// The resolved caller identity.
    pub actor: Actor,
}

/// Answers [`crate::request::ListMyProjects`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectList {
    /// Every project the caller owns or is a member of.
    pub projects: Vec<ProjectMeta>,
}

/// Answers [`crate::request::GetProject`] and the mutating project requests
/// (`PublishProject`, `SetAccess`, `ArchiveProject`, `RestoreProject`,
/// `AddMember`, `RemoveMember`) with the resulting state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectInfo {
    /// Identity and access metadata.
    pub meta: ProjectMeta,
    /// Current head set (normally one entry — see [`PushOutcome::NewHead`]).
    pub heads: Vec<HeadInfo>,
    /// Client-computed display metadata from the most recent commit.
    pub sidecar: SidecarMeta,
    /// Who has been granted access by email, or `None` when the caller is
    /// not entitled to know.
    ///
    /// The member list is a list of people's **email addresses**, so it is
    /// answered to the people on it — the project's members — and to nobody
    /// else. A link-holder gets `None` however much the link grants them:
    /// an [`Access::Edit`](crate::access::Access::Edit) link is write access
    /// to the project, never access to the roster of who else has it.
    pub members: Option<Vec<MemberInfo>>,
}

/// Answers [`crate::request::GetHeads`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Heads {
    /// The project's current head set.
    pub heads: Vec<HeadInfo>,
}

/// Answers [`crate::request::HaveBlobs`]: the subset of the queried hashes
/// the server does not already have.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MissingBlobs {
    /// Hashes the server is missing.
    pub hashes: Vec<ContentHash>,
}

/// Answers [`crate::request::PushCommit`]: the accepted head state. Push is
/// never blocked (D5) — the outcome only says whether the line advanced or
/// gained a sibling head.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PushResult {
    /// Whether the pushed commit advanced the line or created a new head
    /// alongside an existing one.
    pub outcome: PushOutcome,
    /// The project's full head set after accepting the push.
    pub heads: Vec<HeadInfo>,
}

/// Answers [`crate::request::GetEvents`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Events {
    /// Events recorded after the requested `since` sequence number.
    pub events: Vec<HistoryEvent>,
    /// The server event sequence number to pass as `since` on the next call
    /// to continue reading forward with no gap or overlap.
    pub next_since: u64,
}

impl From<UserInfo> for CloudResponse {
    fn from(response: UserInfo) -> Self {
        CloudResponse::UserInfo(response)
    }
}

impl From<ProjectList> for CloudResponse {
    fn from(response: ProjectList) -> Self {
        CloudResponse::ProjectList(response)
    }
}

impl From<ProjectInfo> for CloudResponse {
    fn from(response: ProjectInfo) -> Self {
        CloudResponse::ProjectInfo(response)
    }
}

impl From<Heads> for CloudResponse {
    fn from(response: Heads) -> Self {
        CloudResponse::Heads(response)
    }
}

impl From<MissingBlobs> for CloudResponse {
    fn from(response: MissingBlobs) -> Self {
        CloudResponse::MissingBlobs(response)
    }
}

impl From<PushResult> for CloudResponse {
    fn from(response: PushResult) -> Self {
        CloudResponse::PushResult(response)
    }
}

impl From<Events> for CloudResponse {
    fn from(response: Events) -> Self {
        CloudResponse::Events(response)
    }
}

impl From<MeInfo> for CloudResponse {
    fn from(response: MeInfo) -> Self {
        CloudResponse::MeInfo(response)
    }
}

impl From<SessionList> for CloudResponse {
    fn from(response: SessionList) -> Self {
        CloudResponse::SessionList(response)
    }
}

impl From<Ack> for CloudResponse {
    fn from(response: Ack) -> Self {
        CloudResponse::Ack(response)
    }
}

impl From<LoginOptionsInfo> for CloudResponse {
    fn from(response: LoginOptionsInfo) -> Self {
        CloudResponse::LoginOptionsInfo(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access::Access;
    use crate::member_role::MemberRole;
    use alloc::string::ToString;
    use alloc::vec;
    use lpc_history::{PrefixedUid, UidPrefix};

    fn uid() -> PrefixedUid {
        PrefixedUid::mint(UidPrefix::Project, &[2u8; 16])
    }

    #[test]
    fn serde_round_trip_user_info() {
        let resp = CloudResponse::UserInfo(UserInfo {
            actor: Actor::Anonymous,
        });
        let json = serde_json::to_string(&resp).unwrap();
        let back: CloudResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn serde_round_trip_project_info() {
        let resp = CloudResponse::ProjectInfo(ProjectInfo {
            meta: ProjectMeta {
                uid: uid(),
                slug: "zook-dome".to_string(),
                access: Access::View,
                owner: Actor::Anonymous,
                archived: false,
            },
            heads: vec![HeadInfo {
                tree: ContentHash::of(b"tree"),
                parents: vec![],
            }],
            sidecar: SidecarMeta {
                name: "Zook Dome".to_string(),
                format_version: 4,
                preview_png: None,
            },
            members: Some(vec![MemberInfo {
                email: "yona@example.com".to_string(),
                role: MemberRole::Owner,
                pending: false,
                user: Some(PrefixedUid::mint(UidPrefix::User, &[5u8; 16])),
            }]),
        });
        let json = serde_json::to_string(&resp).unwrap();
        let back: CloudResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }

    /// A caller with no claim on the member list gets a `null` there, not an
    /// empty list: "you may not know" and "nobody has been invited" are
    /// different answers.
    #[test]
    fn serde_round_trip_project_info_without_members() {
        let resp = CloudResponse::ProjectInfo(ProjectInfo {
            meta: ProjectMeta {
                uid: uid(),
                slug: "zook-dome".to_string(),
                access: Access::View,
                owner: Actor::Anonymous,
                archived: true,
            },
            heads: vec![],
            sidecar: SidecarMeta {
                name: "Zook Dome".to_string(),
                format_version: 4,
                preview_png: None,
            },
            members: None,
        });
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""members":null"#), "{json}");
        let back: CloudResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn serde_round_trip_push_result() {
        let resp = CloudResponse::PushResult(PushResult {
            outcome: PushOutcome::NewHead,
            heads: vec![],
        });
        let json = serde_json::to_string(&resp).unwrap();
        let back: CloudResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn serde_round_trip_events() {
        let resp = CloudResponse::Events(Events {
            events: vec![],
            next_since: 7,
        });
        let json = serde_json::to_string(&resp).unwrap();
        let back: CloudResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }

    /// Pinned JSON literal: the deployed format is the contract.
    #[test]
    fn pinned_json_literal() {
        let resp = CloudResponse::MissingBlobs(MissingBlobs {
            hashes: vec![ContentHash::of(b"x")],
        });
        assert_eq!(
            serde_json::to_string(&resp).unwrap(),
            alloc::format!(
                r#"{{"missingBlobs":{{"hashes":["{}"]}}}}"#,
                ContentHash::of(b"x")
            )
        );
    }

    /// `next_since` is the one multi-word field on the response wire, and the
    /// enum's `rename_all` never applied to it. Pinned so a later
    /// `rename_all_fields` cannot silently rename it.
    #[test]
    fn pinned_json_literal_events() {
        let resp = CloudResponse::Events(Events {
            events: vec![],
            next_since: 7,
        });
        assert_eq!(
            serde_json::to_string(&resp).unwrap(),
            r#"{"events":{"events":[],"next_since":7}}"#
        );
    }

    #[test]
    fn serde_round_trip_me_info() {
        let resp = CloudResponse::MeInfo(crate::me_info::MeInfo {
            uid: PrefixedUid::mint(UidPrefix::User, &[4u8; 16]),
            email: "yona@example.com".to_string(),
            display_name: "Yona".to_string(),
            given_name: None,
            family_name: None,
            picture_url: None,
            provider_label: "Google".to_string(),
            created_at: 1.0,
        });
        let json = serde_json::to_string(&resp).unwrap();
        let back: CloudResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn serde_round_trip_ack() {
        let resp = CloudResponse::Ack(crate::ack::Ack);
        let json = serde_json::to_string(&resp).unwrap();
        let back: CloudResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }

    /// The `Ack` family pinned: a unit-struct payload wraps to `null`, not
    /// `{}`, inside the newtype variant.
    #[test]
    fn pinned_json_literal_ack() {
        let resp = CloudResponse::Ack(crate::ack::Ack);
        assert_eq!(serde_json::to_string(&resp).unwrap(), r#"{"ack":null}"#);
    }

    /// The `LoginOptionsInfo` family pinned: multi-word variant name
    /// (`loginOptionsInfo`) alongside its own multi-field payload.
    #[test]
    fn pinned_json_literal_login_options_info() {
        let resp = CloudResponse::LoginOptionsInfo(crate::login_options::LoginOptionsInfo {
            oidc: vec![],
            dev_picker: None,
        });
        assert_eq!(
            serde_json::to_string(&resp).unwrap(),
            r#"{"loginOptionsInfo":{"oidc":[],"devPicker":null}}"#
        );
    }
}
