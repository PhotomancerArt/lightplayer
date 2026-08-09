//! The bare "it happened" response.

use serde::{Deserialize, Serialize};

/// Answers a request whose only job is a side effect with nothing further
/// to report — currently just [`crate::request::RevokeSession`].
///
/// The crate's existing mutating requests (`SetAccess`, `AddMember`,
/// `RemoveMember`, ...) all answer with the resulting
/// [`crate::response::ProjectInfo`] instead of a bare acknowledgement, so
/// there is no acked-project shape to reuse here — a revoked session has no
/// "resulting record" to hand back. `Ack` is the empty response this crate
/// did not need until now; reuse it for any future request in the same
/// shape rather than inventing a second one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ack;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_round_trip() {
        let json = serde_json::to_string(&Ack).unwrap();
        let back: Ack = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Ack);
    }

    /// Pinned JSON literal: the deployed format is the contract. A unit
    /// struct serializes as `null`, not `{}` — the wire form a client's
    /// deserializer must expect.
    #[test]
    fn pinned_json_literal() {
        assert_eq!(serde_json::to_string(&Ack).unwrap(), "null");
    }
}
