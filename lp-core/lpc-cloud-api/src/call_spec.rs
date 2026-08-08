//! Which response answers which request — as a compile-time fact.
//!
//! [`CloudCallSpec`] pairs one request struct with the one response struct it
//! can legally produce. A caller that holds a
//! [`GetProject`](crate::request::GetProject) gets back a
//! [`ProjectInfo`](crate::response::ProjectInfo), not a `CloudResponse` it has
//! to sift through, so the "what if the service answers with the wrong
//! variant" branch exists in exactly one place per request instead of at every
//! call site.
//!
//! # Why hand-written and not a macro
//!
//! Sixteen requests, one impl each, all in this file: the pairing table is
//! greppable, and a reader who wants to know what `PushCommit` answers with
//! reads it here rather than expanding a macro in their head. Revisit if the
//! vocabulary grows several times over.

use crate::ack::Ack;
use crate::login_options::LoginOptionsInfo;
use crate::me_info::MeInfo;
use crate::request::{
    AddMember, CloudRequest, GetEvents, GetHeads, GetMe, GetProject, HaveBlobs, ListMyProjects,
    ListSessions, LoginOptions, PublishProject, PushCommit, RemoveMember, RevokeSession,
    SetVisibility, UpdateMe, WhoAmI,
};
use crate::response::{
    CloudResponse, Events, Heads, MissingBlobs, ProjectInfo, ProjectList, PushResult, UserInfo,
};
use crate::session_info::SessionList;

/// One request type and the one response type that answers it.
///
/// `Into<CloudRequest>` is the supertrait rather than a method, so the
/// envelope-building side never needs this trait in scope to send a request —
/// only the side that has to read the answer does.
pub trait CloudCallSpec: Into<CloudRequest> {
    /// The only response shape this request can legally produce.
    type Response;

    /// Pull this request's response out of a [`CloudResponse`], or `None` if
    /// the answer is some other variant.
    ///
    /// `None` is a protocol violation by whichever side produced it, never a
    /// condition a user can cause; callers report it as such rather than
    /// folding it into the service's own refusal vocabulary
    /// ([`CloudError`](crate::error::CloudError)).
    fn extract(response: CloudResponse) -> Option<Self::Response>;
}

impl CloudCallSpec for WhoAmI {
    type Response = UserInfo;

    fn extract(response: CloudResponse) -> Option<UserInfo> {
        match response {
            CloudResponse::UserInfo(info) => Some(info),
            _ => None,
        }
    }
}

impl CloudCallSpec for ListMyProjects {
    type Response = ProjectList;

    fn extract(response: CloudResponse) -> Option<ProjectList> {
        match response {
            CloudResponse::ProjectList(list) => Some(list),
            _ => None,
        }
    }
}

impl CloudCallSpec for PublishProject {
    type Response = ProjectInfo;

    fn extract(response: CloudResponse) -> Option<ProjectInfo> {
        match response {
            CloudResponse::ProjectInfo(info) => Some(info),
            _ => None,
        }
    }
}

impl CloudCallSpec for SetVisibility {
    type Response = ProjectInfo;

    fn extract(response: CloudResponse) -> Option<ProjectInfo> {
        match response {
            CloudResponse::ProjectInfo(info) => Some(info),
            _ => None,
        }
    }
}

impl CloudCallSpec for AddMember {
    type Response = ProjectInfo;

    fn extract(response: CloudResponse) -> Option<ProjectInfo> {
        match response {
            CloudResponse::ProjectInfo(info) => Some(info),
            _ => None,
        }
    }
}

impl CloudCallSpec for RemoveMember {
    type Response = ProjectInfo;

    fn extract(response: CloudResponse) -> Option<ProjectInfo> {
        match response {
            CloudResponse::ProjectInfo(info) => Some(info),
            _ => None,
        }
    }
}

impl CloudCallSpec for GetProject {
    type Response = ProjectInfo;

    fn extract(response: CloudResponse) -> Option<ProjectInfo> {
        match response {
            CloudResponse::ProjectInfo(info) => Some(info),
            _ => None,
        }
    }
}

impl CloudCallSpec for GetHeads {
    type Response = Heads;

    fn extract(response: CloudResponse) -> Option<Heads> {
        match response {
            CloudResponse::Heads(heads) => Some(heads),
            _ => None,
        }
    }
}

impl CloudCallSpec for HaveBlobs {
    type Response = MissingBlobs;

    fn extract(response: CloudResponse) -> Option<MissingBlobs> {
        match response {
            CloudResponse::MissingBlobs(missing) => Some(missing),
            _ => None,
        }
    }
}

impl CloudCallSpec for PushCommit {
    type Response = PushResult;

    fn extract(response: CloudResponse) -> Option<PushResult> {
        match response {
            CloudResponse::PushResult(result) => Some(result),
            _ => None,
        }
    }
}

impl CloudCallSpec for GetEvents {
    type Response = Events;

    fn extract(response: CloudResponse) -> Option<Events> {
        match response {
            CloudResponse::Events(events) => Some(events),
            _ => None,
        }
    }
}

impl CloudCallSpec for GetMe {
    type Response = MeInfo;

    fn extract(response: CloudResponse) -> Option<MeInfo> {
        match response {
            CloudResponse::MeInfo(info) => Some(info),
            _ => None,
        }
    }
}

impl CloudCallSpec for UpdateMe {
    type Response = MeInfo;

    fn extract(response: CloudResponse) -> Option<MeInfo> {
        match response {
            CloudResponse::MeInfo(info) => Some(info),
            _ => None,
        }
    }
}

impl CloudCallSpec for ListSessions {
    type Response = SessionList;

    fn extract(response: CloudResponse) -> Option<SessionList> {
        match response {
            CloudResponse::SessionList(list) => Some(list),
            _ => None,
        }
    }
}

impl CloudCallSpec for RevokeSession {
    type Response = Ack;

    fn extract(response: CloudResponse) -> Option<Ack> {
        match response {
            CloudResponse::Ack(ack) => Some(ack),
            _ => None,
        }
    }
}

impl CloudCallSpec for LoginOptions {
    type Response = LoginOptionsInfo;

    fn extract(response: CloudResponse) -> Option<LoginOptionsInfo> {
        match response {
            CloudResponse::LoginOptionsInfo(info) => Some(info),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use lpc_history::{ContentHash, PrefixedUid, UidPrefix};

    fn uid() -> PrefixedUid {
        PrefixedUid::mint(UidPrefix::Project, &[3u8; 16])
    }

    #[test]
    fn a_request_converts_to_its_enum_variant() {
        let request: CloudRequest = GetProject { uid: uid() }.into();
        assert_eq!(request, CloudRequest::GetProject(GetProject { uid: uid() }));
    }

    #[test]
    fn extract_returns_the_paired_response() {
        let answer = CloudResponse::Heads(Heads { heads: vec![] });
        assert!(GetHeads::extract(answer).is_some());
    }

    /// The whole point: an answer of the wrong shape is recognizable, not
    /// silently reinterpreted.
    #[test]
    fn extract_refuses_a_different_variant() {
        let answer = CloudResponse::MissingBlobs(MissingBlobs {
            hashes: vec![ContentHash::of(b"x")],
        });
        assert!(GetHeads::extract(answer).is_none());
    }
}
