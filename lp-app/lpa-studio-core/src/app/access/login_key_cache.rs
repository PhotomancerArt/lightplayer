//! Deriving login keys in the client, once per `(salt, iterations,
//! password)` per session.
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
/// 2026-09-24 in headless desktop Chrome (M2 Max) through Studio's own wasm:
/// see `docs/adr/2026-09-24-ble-transport-studio.md` §Consequences for the
/// figure this was scaled from. The desk walk in Bluefy (M7) re-measures
/// and moves this if the phone is slower than the scaling assumed; an
/// installed secret keeps the cost it was written with, so changing this
/// never locks anyone out.
pub const DEFAULT_KDF_ITERATIONS: u32 = 60_000;

/// The session's derived keys.
#[derive(Default)]
pub struct LoginKeyCache {
    keys: HashMap<([u8; SALT_BYTES], u32, String), [u8; KEY_BYTES]>,
}

impl LoginKeyCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The key for `offer` under `password`, from the cache when it has it.
    /// Returns whether it had to derive (the caller yields to the page
    /// between derivations, so a many-offer challenge never freezes it).
    pub fn key_for(&mut self, offer: &LoginOffer, password: &str) -> ([u8; KEY_BYTES], bool) {
        let slot = (offer.salt, offer.iterations, password.to_string());
        if let Some(key) = self.keys.get(&slot) {
            return (*key, false);
        }
        let key = derive_login_key(password.as_bytes(), &offer.salt, offer.iterations);
        self.keys.insert(slot, key);
        (key, true)
    }

    /// Whether `password` under `offer` is already derived.
    pub fn has(&self, offer: &LoginOffer, password: &str) -> bool {
        self.keys
            .contains_key(&(offer.salt, offer.iterations, password.to_string()))
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
