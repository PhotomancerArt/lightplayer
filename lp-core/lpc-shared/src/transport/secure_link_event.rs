//! A secure link's handshake, as a transport reports it to the server and
//! the server answers it.
//!
//! A transport whose links run lp-link's `secure` feature (a device's
//! network link: LAN WebSocket, the relay) raises these from
//! `Link::poll_secure_event` and `Link::session_auth`, and forwards the
//! server's [`KeyAnswer`] to `Link::provide_keys` / `Link::refuse`. Plain
//! arrays, so this crate does not depend on lp-link. A transport with no
//! secure links inherits [`ServerTransport`](super::ServerTransport)'s empty
//! defaults and never sees one.

use alloc::vec::Vec;

/// What a secure link's handshake asks of, or tells, the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecureLinkEvent {
    /// The initiator named the entry with this salt (its key id). Answer
    /// with [`ServerTransport::answer_key_lookup`](super::ServerTransport::answer_key_lookup).
    KeyLookup { salt: [u8; 16] },
    /// A known salt whose PSK matched no candidate: a failed guess, charged
    /// to the device's login backoff.
    WrongKey { salt: [u8; 16] },
    /// The handshake completed and the link is up: it authenticated
    /// `candidate` (an index into the `Keys` the server answered) of the
    /// entry with `salt`. The link's tier is that candidate's.
    Authenticated { salt: [u8; 16], candidate: u8 },
}

/// The server's answer to a [`SecureLinkEvent::KeyLookup`].
#[derive(Clone, PartialEq, Eq)]
pub enum KeyAnswer {
    /// The candidate PSKs, best tier first.
    Keys(Vec<[u8; 32]>),
    /// No entry has this salt (not a strike against the backoff).
    Unknown,
    /// The device is in login backoff: try again after `retry_after_ms`.
    Backoff { retry_after_ms: u32 },
}

impl core::fmt::Debug for KeyAnswer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            // Never print a PSK.
            Self::Keys(keys) => write!(f, "Keys({} candidates)", keys.len()),
            Self::Unknown => f.write_str("Unknown"),
            Self::Backoff { retry_after_ms } => write!(f, "Backoff({retry_after_ms} ms)"),
        }
    }
}
