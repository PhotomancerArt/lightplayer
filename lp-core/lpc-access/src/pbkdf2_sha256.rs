//! PBKDF2 with HMAC-SHA256 as its PRF, written from RFC 8018 §5.2.
//!
//! ```text
//! DK  = T_1 || T_2 || … || T_l            (truncated to dkLen)
//! T_i = U_1 ^ U_2 ^ … ^ U_c
//! U_1 = PRF(P, S || INT(i))                (INT = 4-byte big-endian)
//! U_j = PRF(P, U_{j-1})
//! ```
//!
//! **This runs on the client only** — Studio's wasm and `lp-cli` — never on
//! the board. The board stores the derived key `K` (with its salt and
//! iteration count) and verifies `HMAC(K, challenge)`; spending the KDF's
//! deliberate cost on an ESP32-C6 would buy nothing, because `K` is what a
//! login proves possession of either way. See
//! `docs/adr/2026-09-23-ble-access-model.md`.
//!
//! RustCrypto's `pbkdf2` crate is a dev-dependency oracle for the tests
//! below and is never linked into the product.

use crate::hmac_sha256::{HMAC_SHA256_BYTES, HmacSha256};

/// Derive `out.len()` bytes from `password` and `salt` over `iterations`
/// rounds. `iterations == 0` is treated as 1 (RFC 8018 requires `c >= 1`;
/// the persisted formats refuse 0 before it can get here).
pub fn pbkdf2_sha256(password: &[u8], salt: &[u8], iterations: u32, out: &mut [u8]) {
    let iterations = iterations.max(1);
    // The PRF is always keyed by the password: key it once, clone per use.
    let keyed = HmacSha256::new(password);

    for (block_index, chunk) in out.chunks_mut(HMAC_SHA256_BYTES).enumerate() {
        // Block numbers start at 1; a `u32` covers any output this crate
        // will ever be asked for (2^32 - 1 blocks is RFC 8018's own limit).
        let block_number = (block_index as u32).wrapping_add(1);

        let mut mac = keyed.clone();
        mac.update(salt);
        mac.update(&block_number.to_be_bytes());
        let mut u = mac.finalize();
        let mut t = u;

        for _ in 1..iterations {
            let mut mac = keyed.clone();
            mac.update(&u);
            u = mac.finalize();
            for (t_byte, u_byte) in t.iter_mut().zip(u.iter()) {
                *t_byte ^= u_byte;
            }
        }

        chunk.copy_from_slice(&t[..chunk.len()]);
    }
}

/// Derive the 32-byte login key `K` a [`crate::SecretEntry`] stores.
#[must_use]
pub fn derive_login_key(password: &[u8], salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2_sha256(password, salt, iterations, &mut key);
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    /// The widely published PBKDF2-HMAC-SHA256 vectors (P = "password",
    /// S = "salt", 32-byte output), plus RFC 7914 §11's two 64-byte ones,
    /// which exercise a second output block.
    #[test]
    fn published_vectors() {
        let cases: [(&[u8], &[u8], u32, &str); 5] = [
            (
                b"password",
                b"salt",
                1,
                "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b",
            ),
            (
                b"password",
                b"salt",
                2,
                "ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43",
            ),
            (
                b"password",
                b"salt",
                4096,
                "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a",
            ),
            (
                b"passwd",
                b"salt",
                1,
                "55ac046e56e3089fec1691c22544b605f94185216dde0465e68b9d57c20dacbc\
                 49ca9cccf179b645991664b39d77ef317c71b845b1e30bd509112041d3a19783",
            ),
            (
                b"Password",
                b"NaCl",
                80_000,
                "4ddcd8f60b98be21830cee5ef22701f9641a4418d04c0414aeff08876b34ab56\
                 a1d425a1225833549adb841b51c9b3176a272bdebba1d078478f62b397f33c8d",
            ),
        ];
        for (password, salt, iterations, expected_hex) in cases {
            let expected = hex(expected_hex);
            let mut out = vec![0u8; expected.len()];
            pbkdf2_sha256(password, salt, iterations, &mut out);
            assert_eq!(out, expected, "P={password:?} S={salt:?} c={iterations}");
        }
    }

    /// Output lengths that end mid-block, and salts/passwords of assorted
    /// lengths, agree with RustCrypto's `pbkdf2` (the oracle).
    #[test]
    fn agrees_with_the_rustcrypto_oracle() {
        for out_len in [1usize, 16, 31, 32, 33, 64, 65, 100] {
            for (password, salt, iterations) in [
                (&b""[..], &b""[..], 1u32),
                (b"hunter2", b"0123456789abcdef", 3),
                (
                    b"a password longer than one sixty-four byte sha-256 block, to force hashing",
                    b"salt",
                    7,
                ),
            ] {
                let mut ours = vec![0u8; out_len];
                pbkdf2_sha256(password, salt, iterations, &mut ours);
                let mut oracle = vec![0u8; out_len];
                pbkdf2::pbkdf2_hmac::<sha2::Sha256>(password, salt, iterations, &mut oracle);
                assert_eq!(ours, oracle, "len {out_len}, c={iterations}");
            }
        }
    }

    #[test]
    fn login_key_is_the_32_byte_derivation() {
        let mut full = [0u8; 32];
        pbkdf2_sha256(b"camp", b"0123456789abcdef", 10, &mut full);
        assert_eq!(derive_login_key(b"camp", b"0123456789abcdef", 10), full);
    }

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }
}
