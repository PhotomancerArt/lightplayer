//! Typed answers to every [`crate::request::CloudRequest`].
//!
//! Responses mirror requests one-to-one where the request produces a
//! distinct payload; a request whose only job is to mutate project state
//! (`PublishProject`, `SetVisibility`, `AddMember`, `RemoveMember`) answers
//! with the resulting [`ProjectInfo`] rather than a bare acknowledgement, so
//! the caller never needs a follow-up `GetProject` to see what it just
//! changed.

use alloc::vec::Vec;
use lpc_history::{ContentHash, HistoryEvent};
use serde::{Deserialize, Serialize};

use crate::actor::Actor;
use crate::head_info::{HeadInfo, PushOutcome};
use crate::project_meta::ProjectMeta;
use crate::sidecar_meta::SidecarMeta;

/// A service→client response, carried inside a
/// [`crate::envelope::CloudReply`]'s `Ok` side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloudResponse {
    /// Answers [`crate::request::CloudRequest::WhoAmI`].
    UserInfo {
        /// The resolved caller identity.
        actor: Actor,
    },
    /// Answers [`crate::request::CloudRequest::ListMyProjects`].
    ProjectList {
        /// Every project the caller owns or is a member of.
        projects: Vec<ProjectMeta>,
    },
    /// Answers [`crate::request::CloudRequest::GetProject`] and the
    /// mutating project requests (`PublishProject`, `SetVisibility`,
    /// `AddMember`, `RemoveMember`) with the resulting state.
    ProjectInfo {
        /// Identity and access metadata.
        meta: ProjectMeta,
        /// Current head set (normally one entry — see
        /// [`PushOutcome::NewHead`]).
        heads: Vec<HeadInfo>,
        /// Client-computed display metadata from the most recent commit.
        sidecar: SidecarMeta,
    },
    /// Answers [`crate::request::CloudRequest::GetHeads`].
    Heads {
        /// The project's current head set.
        heads: Vec<HeadInfo>,
    },
    /// Answers [`crate::request::CloudRequest::HaveBlobs`]: the subset of
    /// the queried hashes the server does not already have.
    MissingBlobs {
        /// Hashes the server is missing.
        hashes: Vec<ContentHash>,
    },
    /// Answers [`crate::request::CloudRequest::PushCommit`]: the accepted
    /// head state. Push is never blocked (D5) — the outcome only says
    /// whether the line advanced or gained a sibling head.
    PushResult {
        /// Whether the pushed commit advanced the line or created a new
        /// head alongside an existing one.
        outcome: PushOutcome,
        /// The project's full head set after accepting the push.
        heads: Vec<HeadInfo>,
    },
    /// Answers [`crate::request::CloudRequest::GetEvents`].
    Events {
        /// Events recorded after the requested `since` sequence number.
        events: Vec<HistoryEvent>,
        /// The server event sequence number to pass as `since` on the next
        /// call to continue reading forward with no gap or overlap.
        next_since: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visibility::Visibility;
    use alloc::string::ToString;
    use alloc::vec;
    use lpc_history::{PrefixedUid, UidPrefix};

    fn uid() -> PrefixedUid {
        PrefixedUid::mint(UidPrefix::Project, &[2u8; 16])
    }

    #[test]
    fn serde_round_trip_user_info() {
        let resp = CloudResponse::UserInfo {
            actor: Actor::Anonymous,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: CloudResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn serde_round_trip_project_info() {
        let resp = CloudResponse::ProjectInfo {
            meta: ProjectMeta {
                uid: uid(),
                slug: "zook-dome".to_string(),
                visibility: Visibility::Link,
                owner: Actor::Anonymous,
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
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: CloudResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn serde_round_trip_push_result() {
        let resp = CloudResponse::PushResult {
            outcome: PushOutcome::NewHead,
            heads: vec![],
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: CloudResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }

    #[test]
    fn serde_round_trip_events() {
        let resp = CloudResponse::Events {
            events: vec![],
            next_since: 7,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: CloudResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back, resp);
    }

    /// Pinned JSON literal: the deployed format is the contract.
    #[test]
    fn pinned_json_literal() {
        let resp = CloudResponse::MissingBlobs {
            hashes: vec![ContentHash::of(b"x")],
        };
        assert_eq!(
            serde_json::to_string(&resp).unwrap(),
            alloc::format!(
                r#"{{"missingBlobs":{{"hashes":["{}"]}}}}"#,
                ContentHash::of(b"x")
            )
        );
    }
}
