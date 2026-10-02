//! What a secure link tells its edge beside [`LinkEvent`](crate::LinkEvent):
//! the handshake's questions and outcomes, drained with
//! `Link::poll_secure_event`. `LinkEvent` itself is unchanged (it is matched
//! all over the workspace); `Up` and `Reset` keep their meaning.

use crate::secure_channel::key_id::KeyId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecureEvent {
    /// Responder: an initiator named `key_id`. Answer with
    /// `Link::provide_keys` (its candidate PSKs, best tier first) or
    /// `Link::refuse`. Raised once per msg1; a resent msg1 waits on the same
    /// question. Unanswered for 2 s, the link refuses it `Busy`.
    KeyLookup { key_id: KeyId },
    /// Responder: a known key id whose PSK did not match any candidate. The
    /// initiator was refused [`RefusalReason::WrongKey`]; the edge charges
    /// it to its login backoff (an unknown key tested no secret and is not
    /// charged).
    WrongKey { key_id: KeyId },
    /// Initiator: the responder refused this key. SYNs stop until
    /// `Link::retry_with` (another key) or `Link::restart`.
    Refused {
        reason: RefusalReason,
        retry_after_ms: u32,
    },
    /// Initiator: the other end runs a plain link. It never comes up (there
    /// is no downgrade); raised once per session.
    PeerNotSecure,
}

/// Why a responder refused a key (a refusal SYN's reason byte).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefusalReason {
    /// No entry has this key id.
    UnknownKey = 1,
    /// An entry has this key id, but the PSK did not match.
    WrongKey = 2,
    /// Too many wrong keys lately: try again after `retry_after_ms`.
    Backoff = 3,
    /// The device did not answer the lookup in time.
    Busy = 4,
}

impl RefusalReason {
    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(RefusalReason::UnknownKey),
            2 => Some(RefusalReason::WrongKey),
            3 => Some(RefusalReason::Backoff),
            4 => Some(RefusalReason::Busy),
            _ => None,
        }
    }
}

/// Responder, once Up: which key the session authenticated, and which of the
/// edge's candidates (an index into what it gave `provide_keys`) matched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionAuth {
    pub key_id: KeyId,
    pub candidate: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reason_codes_round_trip() {
        for r in [
            RefusalReason::UnknownKey,
            RefusalReason::WrongKey,
            RefusalReason::Backoff,
            RefusalReason::Busy,
        ] {
            assert_eq!(RefusalReason::from_code(r.code()), Some(r));
        }
        assert_eq!(RefusalReason::from_code(0), None);
        assert_eq!(RefusalReason::from_code(5), None);
    }
}
