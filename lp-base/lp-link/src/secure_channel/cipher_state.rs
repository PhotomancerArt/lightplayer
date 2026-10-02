//! Noise's CipherState over ChaCha20-Poly1305 (RFC 8439), in place, with a
//! detached 16-byte tag and no allocation.
//!
//! Two forms:
//! - [`CipherKey::seal`] / [`CipherKey::open`] take an **explicit** `n`: the
//!   transport phase, where every frame carries its own counter (Noise spec
//!   §11.4, out-of-order transport messages), so a resent or reordered frame
//!   never desynchronises a nonce.
//! - [`CipherState`] keeps Noise's implicit `n` for the handshake
//!   (`EncryptWithAd` / `DecryptWithAd` inside `EncryptAndHash`).
//!
//! The nonce is Noise's ChaChaPoly encoding: 32 bits of zeros followed by the
//! little-endian 64-bit `n`.

use chacha20poly1305::aead::AeadInPlace;
use chacha20poly1305::{ChaCha20Poly1305, KeyInit};
use zeroize::Zeroize;

/// Bytes in a Poly1305 tag.
pub const TAG_LEN: usize = 16;

/// A tag that did not verify: the bytes were forged, damaged past the CRC, or
/// sealed under another key or counter. The buffer's contents are then
/// unspecified (ciphertext, never partial plaintext handed on).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BadTag;

/// A 32-byte ChaCha20-Poly1305 key, wiped when dropped.
pub struct CipherKey([u8; 32]);

impl CipherKey {
    pub fn new(bytes: [u8; 32]) -> Self {
        CipherKey(bytes)
    }

    /// Encrypt `buf` in place under nonce `n` with associated data `ad`; the
    /// tag.
    pub fn seal(&self, n: u64, ad: &[u8], buf: &mut [u8]) -> [u8; TAG_LEN] {
        seal_raw(&self.0, &noise_nonce(n), ad, buf)
    }

    /// Check `tag` and decrypt `buf` in place.
    pub fn open(
        &self,
        n: u64,
        ad: &[u8],
        buf: &mut [u8],
        tag: &[u8; TAG_LEN],
    ) -> Result<(), BadTag> {
        open_raw(&self.0, &noise_nonce(n), ad, buf, tag)
    }

    #[cfg(any(test, feature = "sim"))]
    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Drop for CipherKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// Noise's CipherState for the handshake: an optional key and an implicit
/// nonce counter. With no key, encryption is the identity (the spec's
/// `EncryptWithAd` before any `MixKey`); NNpsk0 sets the key in its very first
/// token, so lp-link's messages are always encrypted.
#[derive(Default)]
pub struct CipherState {
    key: Option<[u8; 32]>,
    n: u64,
}

impl CipherState {
    /// `InitializeKey(k)`: a new key, `n = 0`.
    pub fn initialize_key(&mut self, k: [u8; 32]) {
        if let Some(old) = self.key.as_mut() {
            old.zeroize();
        }
        self.key = Some(k);
        self.n = 0;
    }

    pub fn has_key(&self) -> bool {
        self.key.is_some()
    }

    /// `EncryptWithAd(ad, plaintext)` in place: the tag, or `None` with no key
    /// (nothing was encrypted and nothing is appended).
    pub fn encrypt_with_ad(&mut self, ad: &[u8], buf: &mut [u8]) -> Option<[u8; TAG_LEN]> {
        let key = self.key.as_ref()?;
        let tag = seal_raw(key, &noise_nonce(self.n), ad, buf);
        self.n += 1;
        Some(tag)
    }

    /// `DecryptWithAd(ad, ciphertext)` in place. With no key, `tag` must be
    /// `None`; with a key it must be `Some`.
    pub fn decrypt_with_ad(
        &mut self,
        ad: &[u8],
        buf: &mut [u8],
        tag: Option<&[u8; TAG_LEN]>,
    ) -> Result<(), BadTag> {
        match (self.key.as_ref(), tag) {
            (None, None) => Ok(()),
            (Some(key), Some(tag)) => {
                open_raw(key, &noise_nonce(self.n), ad, buf, tag)?;
                self.n += 1;
                Ok(())
            }
            _ => Err(BadTag),
        }
    }
}

impl Clone for CipherState {
    fn clone(&self) -> Self {
        CipherState {
            key: self.key,
            n: self.n,
        }
    }
}

impl Drop for CipherState {
    fn drop(&mut self) {
        if let Some(k) = self.key.as_mut() {
            k.zeroize();
        }
    }
}

/// 32 zero bits, then `n` little-endian (Noise §12.3, ChaChaPoly).
fn noise_nonce(n: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[4..].copy_from_slice(&n.to_le_bytes());
    nonce
}

fn seal_raw(key: &[u8; 32], nonce: &[u8; 12], ad: &[u8], buf: &mut [u8]) -> [u8; TAG_LEN] {
    let cipher = ChaCha20Poly1305::new(key.into());
    // Only fails for a message longer than ChaCha20's 256 GiB keystream.
    let tag = cipher
        .encrypt_in_place_detached(nonce.into(), ad, buf)
        .unwrap_or_default();
    tag.into()
}

fn open_raw(
    key: &[u8; 32],
    nonce: &[u8; 12],
    ad: &[u8],
    buf: &mut [u8],
    tag: &[u8; TAG_LEN],
) -> Result<(), BadTag> {
    let cipher = ChaCha20Poly1305::new(key.into());
    cipher
        .decrypt_in_place_detached(nonce.into(), ad, buf, tag.into())
        .map_err(|_| BadTag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    const KEY: [u8; 32] = [
        0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x8b, 0x8c, 0x8d, 0x8e,
        0x8f, 0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0x9b, 0x9c, 0x9d,
        0x9e, 0x9f,
    ];
    const AAD: [u8; 12] = [
        0x50, 0x51, 0x52, 0x53, 0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7,
    ];
    const NONCE: [u8; 12] = [
        0x07, 0x00, 0x00, 0x00, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47,
    ];
    const PLAINTEXT: &[u8] = b"Ladies and Gentlemen of the class of '99: \
        If I could offer you only one tip for the future, sunscreen would be it.";
    const CIPHERTEXT: [u8; 114] = [
        0xd3, 0x1a, 0x8d, 0x34, 0x64, 0x8e, 0x60, 0xdb, 0x7b, 0x86, 0xaf, 0xbc, 0x53, 0xef, 0x7e,
        0xc2, 0xa4, 0xad, 0xed, 0x51, 0x29, 0x6e, 0x08, 0xfe, 0xa9, 0xe2, 0xb5, 0xa7, 0x36, 0xee,
        0x62, 0xd6, 0x3d, 0xbe, 0xa4, 0x5e, 0x8c, 0xa9, 0x67, 0x12, 0x82, 0xfa, 0xfb, 0x69, 0xda,
        0x92, 0x72, 0x8b, 0x1a, 0x71, 0xde, 0x0a, 0x9e, 0x06, 0x0b, 0x29, 0x05, 0xd6, 0xa5, 0xb6,
        0x7e, 0xcd, 0x3b, 0x36, 0x92, 0xdd, 0xbd, 0x7f, 0x2d, 0x77, 0x8b, 0x8c, 0x98, 0x03, 0xae,
        0xe3, 0x28, 0x09, 0x1b, 0x58, 0xfa, 0xb3, 0x24, 0xe4, 0xfa, 0xd6, 0x75, 0x94, 0x55, 0x85,
        0x80, 0x8b, 0x48, 0x31, 0xd7, 0xbc, 0x3f, 0xf4, 0xde, 0xf0, 0x8e, 0x4b, 0x7a, 0x9d, 0xe5,
        0x76, 0xd2, 0x65, 0x86, 0xce, 0xc6, 0x4b, 0x61, 0x16,
    ];
    const TAG: [u8; 16] = [
        0x1a, 0xe1, 0x0b, 0x59, 0x4f, 0x09, 0xe2, 0x6a, 0x7e, 0x90, 0x2e, 0xcb, 0xd0, 0x60, 0x06,
        0x91,
    ];

    /// RFC 8439 §2.8.2, through the in-place detached path this module uses.
    #[test]
    fn rfc_8439_section_2_8_2_vector() {
        let mut buf = PLAINTEXT.to_vec();
        let tag = seal_raw(&KEY, &NONCE, &AAD, &mut buf);
        assert_eq!(buf, CIPHERTEXT);
        assert_eq!(tag, TAG);
        assert_eq!(open_raw(&KEY, &NONCE, &AAD, &mut buf, &TAG), Ok(()));
        assert_eq!(buf, PLAINTEXT);
    }

    #[test]
    fn rfc_8439_vector_refuses_any_flipped_bit() {
        for bit in [0usize, 7, 500, 113 * 8 + 7] {
            let mut buf = CIPHERTEXT.to_vec();
            buf[bit / 8] ^= 1 << (bit % 8);
            assert_eq!(open_raw(&KEY, &NONCE, &AAD, &mut buf, &TAG), Err(BadTag));
        }
        let mut ad = AAD;
        ad[0] ^= 1;
        let mut buf = CIPHERTEXT.to_vec();
        assert_eq!(open_raw(&KEY, &NONCE, &ad, &mut buf, &TAG), Err(BadTag));
        let mut tag = TAG;
        tag[15] ^= 0x80;
        assert_eq!(open_raw(&KEY, &NONCE, &AAD, &mut buf, &tag), Err(BadTag));
    }

    /// The explicit-`n` form is the raw AEAD under Noise's nonce encoding.
    #[test]
    fn explicit_n_is_the_noise_nonce() {
        let key = CipherKey::new(KEY);
        for n in [0u64, 1, 0xFFFF_FFFF, 0x0807_0605_0403_0201] {
            let mut a: Vec<u8> = (0..40).collect();
            let mut b = a.clone();
            let tag = key.seal(n, b"hdr!", &mut a);
            let mut nonce = [0u8; 12];
            nonce[4..].copy_from_slice(&n.to_le_bytes());
            assert_eq!(tag, seal_raw(&KEY, &nonce, b"hdr!", &mut b));
            assert_eq!(a, b);
            assert_eq!(key.open(n, b"hdr!", &mut a, &tag), Ok(()));
            assert_eq!(a, (0..40).collect::<Vec<u8>>());
            assert_eq!(key.open(n ^ 1, b"hdr!", &mut b, &tag), Err(BadTag));
        }
    }

    #[test]
    fn handshake_cipher_counts_its_own_nonce() {
        let mut tx = CipherState::default();
        let mut rx = CipherState::default();
        let mut empty: [u8; 0] = [];
        assert_eq!(tx.encrypt_with_ad(b"h", &mut empty), None);
        assert_eq!(rx.decrypt_with_ad(b"h", &mut empty, None), Ok(()));
        tx.initialize_key(KEY);
        rx.initialize_key(KEY);
        for _ in 0..3 {
            let mut m = *b"abcd";
            let tag = tx.encrypt_with_ad(b"h", &mut m).unwrap();
            assert_eq!(rx.decrypt_with_ad(b"h", &mut m, Some(&tag)), Ok(()));
            assert_eq!(&m, b"abcd");
        }
        let mut m = *b"abcd";
        let tag = tx.encrypt_with_ad(b"h", &mut m).unwrap();
        let mut skipped = rx.clone();
        let mut m2 = m;
        let _ = tx.encrypt_with_ad(b"h", &mut m2);
        // Out of step by one: refused.
        skipped.n += 1;
        assert_eq!(
            skipped.decrypt_with_ad(b"h", &mut m, Some(&tag)),
            Err(BadTag)
        );
        assert_eq!(rx.decrypt_with_ad(b"h", &mut m, None), Err(BadTag));
    }
}
