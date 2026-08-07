//! A logged-in session, stored as a hash of its token.

use lpc_history::{ContentHash, PrefixedUid};

/// Length of a session token in bytes (256 bits).
pub const SESSION_TOKEN_LEN: usize = 32;

/// A session row.
///
/// **Only the hash of the token is ever stored.** A stolen database dump
/// must not be a stack of live cookies, so the raw token exists exactly
/// twice: in the browser's cookie and in the reply that set it. Look a
/// session up with [`session_token_hash`].
#[derive(Debug, Clone, PartialEq)]
pub struct SessionRecord {
    /// SHA-256 of the raw token bytes — the row's key.
    pub token_hash: ContentHash,
    /// The account the session authenticates.
    pub user: PrefixedUid,
    /// Expiry, f64 epoch seconds. A session at or past this instant
    /// resolves to `Actor::Anonymous`.
    pub expires_at: f64,
}

/// The storage key for a raw session token.
pub fn session_token_hash(token: &[u8]) -> ContentHash {
    ContentHash::of(token)
}
