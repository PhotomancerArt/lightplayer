//! The harness board's random bytes: for each secure handshake's ephemeral
//! key and each login challenge, where the C6 reads its hardware RNG.
//!
//! Test-grade, not cryptographic: std's per-process random hash keys over a
//! counter that never repeats. What matters for a test is that no two draws
//! are equal (a repeated ephemeral reads to lp-link's responder as a resent
//! msg1 and replays its cached answer — the flake
//! `lpa-server/tests/secure_link_access.rs` documents).

extern crate std;

use core::hash::{BuildHasher, Hasher};
use core::sync::atomic::{AtomicU64, Ordering};
use std::collections::hash_map::RandomState;
use std::sync::OnceLock;

/// Fill `buf` with fresh bytes.
pub fn harness_entropy(buf: &mut [u8]) {
    static DRAWS: AtomicU64 = AtomicU64::new(0);
    static KEYS: OnceLock<RandomState> = OnceLock::new();
    let keys = KEYS.get_or_init(RandomState::new);
    for chunk in buf.chunks_mut(8) {
        let mut hasher = keys.build_hasher();
        hasher.write_u64(DRAWS.fetch_add(1, Ordering::Relaxed));
        let bytes = hasher.finish().to_le_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }
}

/// A fresh lp-link nonce (never 0).
pub fn harness_nonce() -> u32 {
    let mut bytes = [0u8; 4];
    harness_entropy(&mut bytes);
    u32::from_le_bytes(bytes) | 1
}
