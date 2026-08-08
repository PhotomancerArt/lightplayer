//! Who is calling, including which of their own sessions asked.

use lpc_cloud_api::Actor;
use lpc_history::ContentHash;

/// The caller of one [`crate::cloud_service::CloudService::handle`] call.
///
/// `actor` is what every existing handler needs; `session` is new (P2): the
/// hash of the caller's own session token, when the edge that resolved
/// `actor` also knows it.
/// [`ListSessions`](lpc_cloud_api::request::ListSessions) needs it to mark
/// the calling session `current` — the caller cannot report its own session
/// id itself, since the token lives in an HttpOnly cookie it never reads.
///
/// A bare [`Actor`] converts into a session-less `Caller` (`session: None`)
/// via [`From`], and [`CloudService::handle`](crate::cloud_service::CloudService::handle)
/// accepts `impl Into<Caller>` — which is what keeps every existing
/// `handle(actor, request)` call site compiling unchanged. Only the
/// handlers that actually need the session ([`list_sessions`](crate::cloud_service::CloudService::list_sessions),
/// [`revoke_session`](crate::cloud_service::CloudService::revoke_session))
/// ask for a full `Caller`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caller {
    /// The resolved identity making the call.
    pub actor: Actor,
    /// The hash of the session token that authenticated this call, if the
    /// edge supplied one.
    pub session: Option<ContentHash>,
}

impl From<Actor> for Caller {
    fn from(actor: Actor) -> Self {
        Caller {
            actor,
            session: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_history::{PrefixedUid, UidPrefix};

    #[test]
    fn a_bare_actor_converts_to_a_session_less_caller() {
        let actor = Actor::User(PrefixedUid::mint(UidPrefix::User, &[1u8; 16]));
        let caller: Caller = actor.into();
        assert_eq!(caller.actor, actor);
        assert_eq!(caller.session, None);
    }
}
