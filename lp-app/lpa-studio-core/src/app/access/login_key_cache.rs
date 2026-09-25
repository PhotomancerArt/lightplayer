//! Deriving login keys in the client, once per `(salt, iterations,
//! password)` per session.
//!
//! A held key's secret (this browser's, the account's) goes through the
//! same cache as a password: it is derived at one iteration, so its entry is
//! cheap, but it keeps one road for every `K`.
//!
//! The board stores `K = PBKDF2-HMAC-SHA256(password, salt, iterations)` and
//! only ever runs the HMAC; the deliberate cost is paid HERE, in Studio's
//! wasm (`lpc-access`'s own PBKDF2, compiled in). One derivation per offer a
//! challenge carries, so a reconnect — which offers the same salts — must
//! not pay it again: the cache holds each derived key for the session, in
//! memory only (a key is login-equivalent and never persisted from here).

use std::collections::HashMap;

use lpc_access::{KEY_BYTES, LoginOffer, SALT_BYTES, derive_login_key};

/// PBKDF2 cost for NEW secrets Studio installs.
///
/// PQ10's budget is ~150 ms per derivation in Bluefy on an iPhone. Measured
/// 2026-09-24 on the desk (M2 Max), this crate's `derive_login_key` built
/// for wasm with Studio's release profile (opt-level "z", LTO, one codegen
/// unit), best of five: 60 000 iterations took 46 ms in V8 (node 25) and
/// 44 ms in JavaScriptCore (bun 1.1) — the engine Bluefy's WebKit view
/// runs; 100 000 took 77 / 73 ms. 60 000 leaves the phone a 3× margin under
/// the budget. The desk walk in Bluefy (M7) re-measures and moves this if
/// the phone is slower than that; an installed secret keeps the cost it was
/// written with, so changing this never locks anyone out.
pub const DEFAULT_KDF_ITERATIONS: u32 = 60_000;

/// The session's derived keys.
#[derive(Default)]
pub struct LoginKeyCache {
    keys: HashMap<([u8; SALT_BYTES], u32, Vec<u8>), [u8; KEY_BYTES]>,
}

impl LoginKeyCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The key for `offer` under `password`, from the cache when it has it.
    /// Returns whether it had to derive (the caller yields to the page
    /// between derivations, so a many-offer challenge never freezes it).
    pub fn key_for(&mut self, offer: &LoginOffer, password: &str) -> ([u8; KEY_BYTES], bool) {
        self.key_for_material(offer, password.as_bytes())
    }

    /// The key for `offer` under raw secret `material` (a held key's secret,
    /// or a password's bytes).
    pub fn key_for_material(
        &mut self,
        offer: &LoginOffer,
        material: &[u8],
    ) -> ([u8; KEY_BYTES], bool) {
        let slot = (offer.salt, offer.iterations, material.to_vec());
        if let Some(key) = self.keys.get(&slot) {
            return (*key, false);
        }
        let key = derive_login_key(material, &offer.salt, offer.iterations);
        self.keys.insert(slot, key);
        (key, true)
    }

    /// Whether `password` under `offer` is already derived.
    pub fn has(&self, offer: &LoginOffer, password: &str) -> bool {
        self.keys
            .contains_key(&(offer.salt, offer.iterations, password.as_bytes().to_vec()))
    }

    /// Forget every derived key (Settings' "Forget remembered passwords"
    /// forgets these too — a key is as good as the password here).
    pub fn clear(&mut self) {
        self.keys.clear();
    }
}

impl core::fmt::Debug for LoginKeyCache {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LoginKeyCache")
            .field("keys", &self.keys.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_derived_once_per_salt_cost_and_password() {
        let mut cache = LoginKeyCache::new();
        let offer = LoginOffer {
            salt: [7; 16],
            iterations: 3,
        };
        let (first, derived) = cache.key_for(&offer, "camp");
        assert!(derived);
        let (again, derived) = cache.key_for(&offer, "camp");
        assert!(!derived);
        assert_eq!(first, again);
        assert_eq!(first, derive_login_key(b"camp", &[7; 16], 3));
        assert!(!cache.has(&offer, "other"));
        cache.clear();
        assert!(!cache.has(&offer, "camp"));
    }
}
