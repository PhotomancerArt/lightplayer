//! HMAC-SHA256, written from RFC 2104 over the workspace `sha2`:
//!
//! ```text
//! HMAC(K, m) = H((K' ^ opad) || H((K' ^ ipad) || m))
//! ```
//!
//! `K'` is `K` zero-padded to SHA-256's 64-byte block (or `H(K)` padded, for
//! a longer key); `ipad`/`opad` are `0x36`/`0x5c` repeated. lpc-access has its
//! own copy (lp-base cannot depend on lp-core); deduplicating the two is a
//! measured follow-up. RustCrypto's `hmac` is a dev-dependency oracle only.

use sha2::{Digest, Sha256};
use zeroize::Zeroize;

const BLOCK: usize = 64;

/// HMAC-SHA256 of the concatenation of `parts` under `key`.
pub fn hmac_sha256(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut block = [0u8; BLOCK];
    if key.len() > BLOCK {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut pad = [0u8; BLOCK];
    for (p, b) in pad.iter_mut().zip(block.iter()) {
        *p = b ^ 0x36;
    }
    let mut inner = Sha256::new();
    inner.update(pad);
    for part in parts {
        inner.update(part);
    }
    let mut inner_digest: [u8; 32] = inner.finalize().into();
    for (p, b) in pad.iter_mut().zip(block.iter()) {
        *p = b ^ 0x5c;
    }
    let mut outer = Sha256::new();
    outer.update(pad);
    outer.update(inner_digest);
    let out = outer.finalize().into();
    block.zeroize();
    pad.zeroize();
    inner_digest.zeroize();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use hmac::{Hmac, Mac};

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// RFC 4231 §4.2 (test case 1) and §4.3 (test case 2).
    #[test]
    fn rfc_4231_vectors() {
        assert_eq!(
            hmac_sha256(&[0x0b; 20], &[b"Hi There"]).to_vec(),
            hex("b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7")
        );
        assert_eq!(
            hmac_sha256(b"Jefe", &[b"what do ya want ", b"for nothing?"]).to_vec(),
            hex("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843")
        );
    }

    /// RFC 4231 §4.7 (test case 6): a key longer than one block is hashed.
    #[test]
    fn long_key_is_hashed_first() {
        let key = [0xaa; 131];
        assert_eq!(
            hmac_sha256(
                &key,
                &[b"Test Using Larger Than Block-Size Key - Hash Key First"]
            )
            .to_vec(),
            hex("60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54")
        );
    }

    #[test]
    fn agrees_with_the_hmac_oracle() {
        for len in [0usize, 1, 31, 32, 33, 63, 64, 65, 200] {
            let key: Vec<u8> = (0..len).map(|i| (i * 7 + 1) as u8).collect();
            let msg: Vec<u8> = (0..len * 3).map(|i| (i * 13 + 5) as u8).collect();
            let mut oracle = Hmac::<Sha256>::new_from_slice(&key).unwrap();
            oracle.update(&msg);
            let (a, b) = msg.split_at(msg.len() / 2);
            assert_eq!(
                hmac_sha256(&key, &[a, b]).to_vec(),
                oracle.finalize().into_bytes().to_vec(),
                "key length {len}"
            );
        }
    }
}
