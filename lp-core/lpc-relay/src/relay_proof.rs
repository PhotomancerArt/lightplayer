//! How a board proves it holds an account's key, without sending it.
//!
//! ```text
//! relay_auth_key(K)            = HMAC-SHA256(K, "lp-relay auth/1")
//! relay_proof(A, nonce, mac)   = HMAC-SHA256(A, nonce ‖ mac)
//! ```
//!
//! `K` is the account entry's key exactly as the board stores it
//! (`SecretEntry::k`, which Studio installs as `PBKDF2(key_secret,
//! key_salt, 1)`). The cloud minted that key and keeps it, so it can
//! compute the same `K` and check the proof; nothing secret crosses the
//! wire.
//!
//! - **Domain separation.** The label keeps the proof key apart from
//!   `link_psk(K) = HMAC(K, "lp-link psk/1")` and from the HMAC login (which
//!   keys HMAC with `K` directly over a nonce), so a relay proof can never
//!   stand in for a link PSK or a login answer, and the other way round.
//! - **The MAC is bound in**, so a proof made for one board cannot be
//!   replayed for another under the same challenge.
//! - **What it proves:** "I hold account A's key." Not "I am board X": a MAC
//!   is public, so any board holding A's key could claim X's. That lets one
//!   account's boards confuse each other at the hub, nothing more — every
//!   browser session is still sealed and keyed end to end, so a wrong board
//!   fails the handshake rather than leaking. Per-device keys would close it.

use lpc_access::{KEY_BYTES, constant_time_eq, hmac_sha256};

/// The label that domain-separates the relay proof key.
pub const RELAY_AUTH_LABEL: &[u8] = b"lp-relay auth/1";

/// The hub's challenge length.
pub const RELAY_NONCE_BYTES: usize = 32;

/// One proof's length.
pub const RELAY_PROOF_BYTES: usize = 32;

/// The proof key for an account entry whose key is `k`.
#[must_use]
pub fn relay_auth_key(k: &[u8; KEY_BYTES]) -> [u8; 32] {
    hmac_sha256(k, RELAY_AUTH_LABEL)
}

/// The proof for one account, under the hub's `nonce`, for the board whose
/// MAC is `board_mac`.
#[must_use]
pub fn relay_proof(
    auth_key: &[u8; 32],
    nonce: &[u8; RELAY_NONCE_BYTES],
    board_mac: &[u8; 6],
) -> [u8; RELAY_PROOF_BYTES] {
    let mut message = [0u8; RELAY_NONCE_BYTES + 6];
    message[..RELAY_NONCE_BYTES].copy_from_slice(nonce);
    message[RELAY_NONCE_BYTES..].copy_from_slice(board_mac);
    hmac_sha256(auth_key, &message)
}

/// The hub's check: whether `proof` is the proof for the entry key `k`.
/// Constant time in where the bytes differ.
#[must_use]
pub fn verify_relay_proof(
    k: &[u8; KEY_BYTES],
    nonce: &[u8; RELAY_NONCE_BYTES],
    board_mac: &[u8; 6],
    proof: &[u8; RELAY_PROOF_BYTES],
) -> bool {
    let expected = relay_proof(&relay_auth_key(k), nonce, board_mac);
    constant_time_eq(&expected, proof)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    /// Computed independently with RustCrypto's `hmac`: the oracle.
    #[test]
    fn the_proof_is_the_documented_hmac_chain() {
        let k = [0x5a; 32];
        let nonce = [0x11; 32];
        let mac = [0x10, 0xbd, 0xa3, 0xb0, 0x8e, 0x30];

        let mut auth = Hmac::<Sha256>::new_from_slice(&k).unwrap();
        auth.update(b"lp-relay auth/1");
        let auth: [u8; 32] = auth.finalize().into_bytes().into();
        let mut proof = Hmac::<Sha256>::new_from_slice(&auth).unwrap();
        proof.update(&nonce);
        proof.update(&mac);
        let expected: [u8; 32] = proof.finalize().into_bytes().into();

        assert_eq!(relay_auth_key(&k), auth);
        assert_eq!(relay_proof(&auth, &nonce, &mac), expected);
        assert!(verify_relay_proof(&k, &nonce, &mac, &expected));
    }

    #[test]
    fn a_proof_is_bound_to_its_key_nonce_and_board() {
        let k = [1; 32];
        let nonce = [2; 32];
        let mac = [3; 6];
        let proof = relay_proof(&relay_auth_key(&k), &nonce, &mac);

        assert!(verify_relay_proof(&k, &nonce, &mac, &proof));
        assert!(!verify_relay_proof(&[9; 32], &nonce, &mac, &proof), "key");
        assert!(!verify_relay_proof(&k, &[9; 32], &mac, &proof), "nonce");
        assert!(!verify_relay_proof(&k, &nonce, &[9; 6], &proof), "board");
    }

    #[test]
    fn the_proof_key_is_not_the_link_psk() {
        let k = [7; 32];
        assert_ne!(relay_auth_key(&k), lpc_access::link_psk(&k));
        assert_ne!(relay_auth_key(&k), k);
    }
}
