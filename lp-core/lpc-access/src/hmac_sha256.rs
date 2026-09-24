//! HMAC-SHA256, written from RFC 2104 over the workspace `sha2`.
//!
//! ```text
//! HMAC(K, m) = H((K' ^ opad) || H((K' ^ ipad) || m))
//! ```
//!
//! where `K'` is `K` zero-padded to the hash's block size (64 bytes for
//! SHA-256), or `H(K)` zero-padded when `K` is longer than a block, and
//! `ipad`/`opad` are the bytes `0x36`/`0x5c` repeated.
//!
//! This is the one primitive the BOARD runs: verifying a login answer is a
//! handful of HMACs over a 32-byte challenge. The KDF that turns a password
//! into the key ([`crate::pbkdf2_sha256`]) runs only on the client. RustCrypto's
//! `hmac` crate is a dev-dependency oracle for the tests below and is never
//! linked into the product.

use sha2::{Digest, Sha256};

/// SHA-256's block size in bytes (RFC 2104's `B`).
const BLOCK_BYTES: usize = 64;

/// Length of an HMAC-SHA256 output (RFC 2104's `L`).
pub const HMAC_SHA256_BYTES: usize = 32;

const IPAD: u8 = 0x36;
const OPAD: u8 = 0x5c;

/// One-shot HMAC-SHA256 of `message` under `key`.
#[must_use]
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; HMAC_SHA256_BYTES] {
    let mut mac = HmacSha256::new(key);
    mac.update(message);
    mac.finalize()
}

/// An HMAC-SHA256 computation keyed once and fed incrementally.
///
/// Keying costs two compression-function calls (the padded key through the
/// inner and the outer hash). Cloning a keyed value reuses that work, which
/// is what makes PBKDF2's thousands of iterations affordable: the key is the
/// same password every round.
#[derive(Clone)]
pub struct HmacSha256 {
    inner: Sha256,
    outer: Sha256,
}

impl HmacSha256 {
    /// Key a new HMAC computation (RFC 2104 steps 1–2, both pads).
    #[must_use]
    pub fn new(key: &[u8]) -> Self {
        let mut block = [0u8; BLOCK_BYTES];
        if key.len() > BLOCK_BYTES {
            let digest = Sha256::digest(key);
            block[..digest.len()].copy_from_slice(&digest);
        } else {
            block[..key.len()].copy_from_slice(key);
        }

        let mut inner_pad = [0u8; BLOCK_BYTES];
        let mut outer_pad = [0u8; BLOCK_BYTES];
        for (index, byte) in block.iter().enumerate() {
            inner_pad[index] = byte ^ IPAD;
            outer_pad[index] = byte ^ OPAD;
        }

        let mut inner = Sha256::new();
        inner.update(inner_pad);
        let mut outer = Sha256::new();
        outer.update(outer_pad);
        Self { inner, outer }
    }

    /// Append message bytes.
    pub fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    /// Finish: the outer hash over the inner digest.
    #[must_use]
    pub fn finalize(self) -> [u8; HMAC_SHA256_BYTES] {
        let Self { inner, mut outer } = self;
        outer.update(inner.finalize());
        outer.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    /// RFC 4231 §4 test cases 1–7 for HMAC-SHA-256. Case 5 is a truncated
    /// output; its full-length answer is checked against its 128-bit prefix.
    #[test]
    fn rfc_4231_vectors() {
        for case in rfc_4231_cases() {
            let mac = hmac_sha256(&case.key, &case.data);
            assert_eq!(
                &mac[..case.expected.len()],
                &case.expected[..],
                "RFC 4231 case {}",
                case.number
            );
        }
    }

    /// The same vectors, and a spread of key and message lengths around the
    /// block boundary, agree with RustCrypto's `hmac` (the oracle).
    #[test]
    fn agrees_with_the_rustcrypto_oracle() {
        use hmac::Mac;
        type Oracle = hmac::Hmac<Sha256>;

        for key_len in [0usize, 1, 31, 32, 63, 64, 65, 100, 131, 200] {
            for message_len in [0usize, 1, 32, 55, 56, 63, 64, 65, 128, 1000] {
                let key: Vec<u8> = (0..key_len).map(|i| (i * 7 + 3) as u8).collect();
                let message: Vec<u8> = (0..message_len).map(|i| (i * 13 + 1) as u8).collect();

                let mut oracle = Oracle::new_from_slice(&key).unwrap();
                oracle.update(&message);
                let expected: [u8; 32] = oracle.finalize().into_bytes().into();

                assert_eq!(
                    hmac_sha256(&key, &message),
                    expected,
                    "key {key_len} B, message {message_len} B"
                );
            }
        }
    }

    /// Feeding the message in pieces gives the one-shot answer, and a
    /// cloned keyed state is an independent computation.
    #[test]
    fn incremental_and_cloned_states_agree() {
        let key = b"key";
        let mut split = HmacSha256::new(key);
        let fresh = split.clone();
        split.update(b"The quick brown fox ");
        split.update(b"jumps over the lazy dog");
        assert_eq!(
            split.finalize(),
            hmac_sha256(key, b"The quick brown fox jumps over the lazy dog")
        );

        let mut other = fresh;
        other.update(b"");
        assert_eq!(other.finalize(), hmac_sha256(key, b""));
    }

    struct Rfc4231Case {
        number: u32,
        key: Vec<u8>,
        data: Vec<u8>,
        expected: Vec<u8>,
    }

    fn rfc_4231_cases() -> Vec<Rfc4231Case> {
        vec![
            Rfc4231Case {
                number: 1,
                key: vec![0x0b; 20],
                data: b"Hi There".to_vec(),
                expected: hex("b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"),
            },
            Rfc4231Case {
                number: 2,
                key: b"Jefe".to_vec(),
                data: b"what do ya want for nothing?".to_vec(),
                expected: hex("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"),
            },
            Rfc4231Case {
                number: 3,
                key: vec![0xaa; 20],
                data: vec![0xdd; 50],
                expected: hex("773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe"),
            },
            Rfc4231Case {
                number: 4,
                key: hex("0102030405060708090a0b0c0d0e0f10111213141516171819"),
                data: vec![0xcd; 50],
                expected: hex("82558a389a443c0ea4cc819899f2083a85f0faa3e578f8077a2e3ff46729665b"),
            },
            Rfc4231Case {
                number: 5,
                key: vec![0x0c; 20],
                data: b"Test With Truncation".to_vec(),
                expected: hex("a3b6167473100ee06e0c796c2955552b"),
            },
            Rfc4231Case {
                number: 6,
                key: vec![0xaa; 131],
                data: b"Test Using Larger Than Block-Size Key - Hash Key First".to_vec(),
                expected: hex("60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"),
            },
            Rfc4231Case {
                number: 7,
                key: vec![0xaa; 131],
                data: b"This is a test using a larger than block-size key and a larger than \
                        block-size data. The key needs to be hashed before being used by the \
                        HMAC algorithm."
                    .to_vec(),
                expected: hex("9b09ffa71b942fcb27635fbcd5b0e944bfdc63644f0713938a7f51535c3a35e2"),
            },
        ]
    }

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }
}
