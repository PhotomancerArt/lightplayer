//! Which accounts a board registering with the relay proved it holds.
//!
//! A board's hello names each account key it holds by salt; it answers the
//! hub's challenge with one proof per salt
//! ([`lpc_relay::relay_proof`]). Here each salt is looked up
//! ([`MetaStore::account_by_key_salt`]: the **current** key only, so a key
//! the account has reset proves nothing) and each proof checked against
//! that account's [`device_key`](crate::AccountAccess::device_key). Pure: the
//! edge runs it once per registration, under the store lock, and nothing
//! else on the relay's path touches the store.

use alloc::vec::Vec;
use lpc_history::PrefixedUid;
use lpc_relay::{RELAY_NONCE_BYTES, RELAY_PROOF_BYTES, verify_relay_proof};

use crate::ports::meta_store::MetaStore;

/// The verdict on one registration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BoardAccounts {
    /// The accounts the board proved, each once, in hello order.
    pub users: Vec<PrefixedUid>,
    /// Bit `i` set when the hello's account `i` verified (what
    /// `Registered.accounts_ok` carries).
    pub accounts_ok: u8,
}

impl BoardAccounts {
    /// Whether no account verified: the hub refuses the board
    /// `UnknownAccount`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.users.is_empty()
    }
}

/// Check every proof of a board's registration. A salt with no proof, a
/// proof with no salt, an unknown or retired salt and a wrong proof all
/// simply do not verify.
pub fn verify_board_accounts<S: MetaStore + ?Sized>(
    store: &S,
    board_mac: &[u8; 6],
    nonce: &[u8; RELAY_NONCE_BYTES],
    salts: &[[u8; 16]],
    proofs: &[[u8; RELAY_PROOF_BYTES]],
) -> BoardAccounts {
    let mut verdict = BoardAccounts::default();
    for (index, (salt, proof)) in salts.iter().zip(proofs).enumerate().take(8) {
        let Some(access) = store.account_by_key_salt(salt) else {
            continue;
        };
        if !verify_relay_proof(&access.device_key(), nonce, board_mac, proof) {
            continue;
        }
        verdict.accounts_ok |= 1 << index;
        if !verdict.users.contains(&access.user) {
            verdict.users.push(access.user);
        }
    }
    verdict
}
