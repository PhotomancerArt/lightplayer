//! An account on the cloud service.

use alloc::string::String;
use lpc_history::PrefixedUid;

/// A user account, identified by their Google `sub` (identity) rather than
/// their email (which can change).
///
/// The uid is minted with [`lpc_history::UidPrefix::User`] (`usr…`) from
/// random bytes supplied by the [`crate::ports::id_mint::IdMint`] port — the
/// domain never generates randomness itself.
#[derive(Debug, Clone, PartialEq)]
pub struct CloudUser {
    /// The account's uid (`usr…`).
    pub uid: PrefixedUid,
    /// Google's stable subject identifier. Identity lives here, not in
    /// `email`, because a Google account can change its address.
    pub google_sub: String,
    /// Verified email address, normalized to lowercase. Matched against
    /// pending membership rows at first login (Q4).
    pub email: String,
    /// Display name, as Google reported it.
    pub display_name: String,
    /// When the account was first seen, f64 epoch seconds from the clock
    /// port.
    pub created_at: f64,
}
