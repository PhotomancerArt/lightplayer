//! The PSK a secure lp-link session runs under, derived from an access
//! entry's key.
//!
//! ```text
//! link_psk(K) = HMAC-SHA256(K, "lp-link psk/1")
//! ```
//!
//! Both ends derive it: the client from the `K` it holds (a browser key, an
//! account key, or a password's PBKDF2 output), the device from the `K` its
//! access entry stores. The label separates it from the login MAC, which
//! keys HMAC with `K` directly over a challenge nonce, so a secure session
//! and an HMAC login never share a key, and a PSK learned one way says
//! nothing about the other. The `/1` names this derivation, so a second one
//! can never be confused with it.
//!
//! The entry's salt is the key id the client sends in the clear; see
//! [`crate::key_lookup`].

use crate::hmac_sha256::hmac_sha256;
use crate::secret_entry::KEY_BYTES;

/// The HMAC message that domain-separates a link PSK from a login MAC.
pub const LINK_PSK_LABEL: &[u8] = b"lp-link psk/1";

/// The secure-link PSK for an entry whose key is `k`.
#[must_use]
pub fn link_psk(k: &[u8; KEY_BYTES]) -> [u8; 32] {
    hmac_sha256(k, LINK_PSK_LABEL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    #[test]
    fn link_psk_is_hmac_of_the_label_under_k() {
        let k = [0x5a; 32];
        let mut oracle = Hmac::<Sha256>::new_from_slice(&k).unwrap();
        oracle.update(b"lp-link psk/1");
        assert_eq!(
            link_psk(&k).to_vec(),
            oracle.finalize().into_bytes().to_vec()
        );
    }

    #[test]
    fn link_psk_is_not_the_login_mac_of_anything_obvious() {
        let k = [7; 32];
        assert_ne!(link_psk(&k), hmac_sha256(&k, &[0; 32]));
        assert_ne!(link_psk(&k), k);
    }
}
