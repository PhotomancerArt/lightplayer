//! Who is making a call.

use lpc_history::PrefixedUid;
use serde::{Deserialize, Serialize};

/// The caller of a [`crate::request::CloudRequest`], resolved by the server
/// edge (session/auth lookup) and handed to domain logic already resolved.
///
/// Lives here, not in a server crate, so client-side tests (and any future
/// offline/preview logic) can share the same type without depending on
/// server internals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Actor {
    /// No authenticated session. Valid for every request that a project's
    /// [`Access`](crate::access::Access) opens to link-holders: reads on a
    /// `View` project, reads *and* writes on an `Edit` one.
    Anonymous,
    /// An authenticated user, identified by their account uid.
    User(PrefixedUid),
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_history::UidPrefix;

    #[test]
    fn serde_round_trip_anonymous() {
        let json = serde_json::to_string(&Actor::Anonymous).unwrap();
        let back: Actor = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Actor::Anonymous);
    }

    #[test]
    fn serde_round_trip_user() {
        let uid = PrefixedUid::mint(UidPrefix::Device, &[3u8; 16]);
        let actor = Actor::User(uid);
        let json = serde_json::to_string(&actor).unwrap();
        let back: Actor = serde_json::from_str(&json).unwrap();
        assert_eq!(back, actor);
    }

    /// Pinned JSON literal: the deployed format is the contract.
    #[test]
    fn pinned_json_literal() {
        assert_eq!(
            serde_json::to_string(&Actor::Anonymous).unwrap(),
            "\"anonymous\""
        );
        let uid = PrefixedUid::mint(UidPrefix::Device, &[0u8; 16]);
        assert_eq!(
            serde_json::to_string(&Actor::User(uid)).unwrap(),
            "{\"user\":\"dev0000000000000000\"}"
        );
    }
}
