//! Randomness, injected.

use crate::model::session_record::SESSION_TOKEN_LEN;

/// The source of random bytes for uid minting and session tokens.
///
/// Same posture as `lpc-history`'s uid minting: the *caller* owns
/// randomness, and the domain only ever consumes bytes it was handed. That
/// is what keeps this crate free of an rng dependency, and what lets a test
/// mint a predictable `usr` uid without stubbing a global.
///
/// An adapter backing this in production must return
/// cryptographically-secure bytes — [`IdMint::session_token`] is a bearer
/// credential, and a guessable one is a login bypass.
pub trait IdMint {
    /// 16 fresh random bytes, for minting a [`lpc_history::PrefixedUid`].
    fn uid_bytes(&mut self) -> [u8; 16];

    /// A fresh random session token (256 bits).
    fn session_token(&mut self) -> [u8; SESSION_TOKEN_LEN];

    /// Fill `out` with fresh random bytes — the account device key's secret
    /// and salts ([`crate::model::account_access::AccountAccess`]). The
    /// secret is a credential that unlocks devices, so the same
    /// cryptographic-quality rule as [`session_token`](Self::session_token)
    /// applies.
    fn random_bytes(&mut self, out: &mut [u8]);
}
