//! Noise's SymmetricState and the NNpsk0 pattern, both roles, written from
//! the Noise Protocol Framework spec (rev 34), §5 (processing rules) and §9
//! (PSK handshakes):
//!
//! ```text
//! Noise_NNpsk0_25519_ChaChaPoly_SHA256
//!   -> psk, e        initiator: MixKeyAndHash(psk); e; MixHash(e.pub); MixKey(e.pub);
//!                               EncryptAndHash(payload = empty)
//!   <- e, ee         responder: e; MixHash(e.pub); MixKey(e.pub); MixKey(DH(e, re));
//!                               EncryptAndHash(payload = responder's lp-link nonce)
//!   Split()          (k1 = initiator → responder, k2 = responder → initiator)
//! ```
//!
//! The `MixKey(e.pub)` after each `e` is §9.2's rule for handshakes with a
//! `psk` modifier. The responder's [`Responder::read_msg1`] does no DH (the
//! psk token and the tag check are all HKDFs and one AEAD), so the device can
//! try every candidate PSK for a key id cheaply; only the winner pays for
//! [`ResponderReady::write_msg2`]'s two scalar multiplications.
//!
//! Ephemeral secrets are caller-supplied bytes (the edge's entropy), used once.
//! Low-order DH outputs are not rejected, as the spec recommends for 25519
//! (§12.1): the psk keys the very first message, so a key-less peer cannot
//! complete a handshake whatever point it sends.

use sha2::{Digest, Sha256};
use x25519_dalek::{X25519_BASEPOINT_BYTES, x25519};
use zeroize::Zeroize;

use crate::secure_channel::cipher_state::{CipherState, TAG_LEN};
use crate::secure_channel::hkdf_sha256::{hkdf2, hkdf3};
use crate::secure_channel::key_id::KeyId;
use crate::secure_channel::psk::Psk;

/// The protocol name hashed into `h` and `ck` first (§5.2).
pub const PROTOCOL_NAME: &[u8] = b"Noise_NNpsk0_25519_ChaChaPoly_SHA256";

/// What the prologue starts with; then the key id and the initiator's nonce.
pub const PROLOGUE_TAG: &[u8; 16] = b"lp-link/secure/1";

/// Bytes in a prologue: tag ‖ key id ‖ initiator nonce (LE u32).
pub const PROLOGUE_LEN: usize = 16 + 16 + 4;

const DH_LEN: usize = 32;

/// msg1 on the wire after the key id: `e_i[32] ‖ tag[16]` (empty payload).
pub const MSG1_LEN: usize = DH_LEN + TAG_LEN;

/// msg2's payload: the responder's lp-link nonce (LE u32).
pub const MSG2_PAYLOAD_LEN: usize = 4;

/// msg2 on the wire: `e_r[32] ‖ enc(nonce)[4] ‖ tag[16]`.
pub const MSG2_LEN: usize = DH_LEN + MSG2_PAYLOAD_LEN + TAG_LEN;

/// The prologue both ends hash before msg1: one lp-link session's identity
/// (which key, which initiator session), so a Noise session cannot be lifted
/// into another lp-link session.
pub fn prologue(key_id: &KeyId, initiator_nonce: u32) -> [u8; PROLOGUE_LEN] {
    let mut p = [0u8; PROLOGUE_LEN];
    p[..16].copy_from_slice(PROLOGUE_TAG);
    p[16..32].copy_from_slice(&key_id.0);
    p[32..].copy_from_slice(&initiator_nonce.to_le_bytes());
    p
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandshakeError {
    /// A message of the wrong length.
    Length,
    /// The tag did not verify: a wrong PSK, or a forged or damaged message.
    BadTag,
}

/// The session's keys after `Split()`, from this end's point of view, and the
/// handshake hash (channel binding). Wiped when dropped; never printed.
pub struct TransportKeys {
    pub send: [u8; 32],
    pub recv: [u8; 32],
    pub handshake_hash: [u8; 32],
}

impl Drop for TransportKeys {
    fn drop(&mut self) {
        self.send.zeroize();
        self.recv.zeroize();
        self.handshake_hash.zeroize();
    }
}

impl core::fmt::Debug for TransportKeys {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("TransportKeys(..)")
    }
}

/// The initiator (the client): built with its key, writes msg1 once, reads
/// msg2.
pub struct Initiator {
    ss: SymmetricState,
    e: EphemeralSecret,
    msg1: [u8; MSG1_LEN],
}

impl Initiator {
    /// Start a handshake and write msg1 (`psk, e`, empty payload). `e_secret`
    /// is 32 fresh random bytes (used once).
    pub fn new(prologue: &[u8], psk: &Psk, e_secret: [u8; 32]) -> Self {
        let mut ss = SymmetricState::new(prologue);
        ss.mix_key_and_hash(psk.as_bytes());
        let e = EphemeralSecret(e_secret);
        let e_pub = e.public();
        ss.mix_hash(&e_pub);
        ss.mix_key(&e_pub);
        let mut msg1 = [0u8; MSG1_LEN];
        msg1[..DH_LEN].copy_from_slice(&e_pub);
        let tag = ss.encrypt_and_hash(&mut []);
        msg1[DH_LEN..].copy_from_slice(&tag);
        Initiator { ss, e, msg1 }
    }

    /// msg1's bytes (the same every time: a resend is identical).
    pub fn msg1(&self) -> &[u8; MSG1_LEN] {
        &self.msg1
    }

    /// Read msg2 (`e, ee`, its payload decrypted into `payload`) and split.
    /// The initiator is left as it was, so a forged or damaged msg2 is
    /// dropped and the genuine one can still be read.
    pub fn read_msg2(
        &self,
        msg: &[u8],
        payload: &mut [u8],
    ) -> Result<TransportKeys, HandshakeError> {
        let mut ss = self.ss.clone();
        let e = &self.e;
        if msg.len() != DH_LEN + payload.len() + TAG_LEN {
            return Err(HandshakeError::Length);
        }
        let mut re = [0u8; DH_LEN];
        re.copy_from_slice(&msg[..DH_LEN]);
        ss.mix_hash(&re);
        ss.mix_key(&re);
        let mut shared = e.dh(re);
        ss.mix_key(&shared);
        shared.zeroize();
        let body = &msg[DH_LEN..msg.len() - TAG_LEN];
        let mut tag = [0u8; TAG_LEN];
        tag.copy_from_slice(&msg[msg.len() - TAG_LEN..]);
        payload.copy_from_slice(body);
        if ss.decrypt_and_hash(payload, &tag).is_err() {
            payload.zeroize();
            return Err(HandshakeError::BadTag);
        }
        let (k1, k2) = ss.split();
        Ok(TransportKeys {
            send: k1,
            recv: k2,
            handshake_hash: ss.h,
        })
    }
}

/// The responder (the device) before msg1: the prologue hashed, nothing else.
pub struct Responder {
    ss: SymmetricState,
}

impl Responder {
    pub fn new(prologue: &[u8]) -> Self {
        Responder {
            ss: SymmetricState::new(prologue),
        }
    }

    /// Read msg1 under `psk`. Cheap (no DH) and side-effect free, so it is
    /// tried once per candidate PSK.
    pub fn read_msg1(&self, msg: &[u8], psk: &Psk) -> Result<ResponderReady, HandshakeError> {
        if msg.len() != MSG1_LEN {
            return Err(HandshakeError::Length);
        }
        let mut ss = self.ss.clone();
        ss.mix_key_and_hash(psk.as_bytes());
        let mut re = [0u8; DH_LEN];
        re.copy_from_slice(&msg[..DH_LEN]);
        ss.mix_hash(&re);
        ss.mix_key(&re);
        let mut tag = [0u8; TAG_LEN];
        tag.copy_from_slice(&msg[DH_LEN..]);
        ss.decrypt_and_hash(&mut [], &tag)
            .map_err(|_| HandshakeError::BadTag)?;
        Ok(ResponderReady { ss, re })
    }
}

/// The responder after a msg1 that verified: writes msg2 and splits.
pub struct ResponderReady {
    ss: SymmetricState,
    re: [u8; DH_LEN],
}

impl ResponderReady {
    /// Write msg2 (`e, ee`, `payload` encrypted) into `out` and split.
    /// `e_secret` is 32 fresh random bytes (used once).
    pub fn write_msg2(
        self,
        e_secret: [u8; 32],
        payload: &[u8],
        out: &mut [u8],
    ) -> Result<TransportKeys, HandshakeError> {
        let ResponderReady { mut ss, re } = self;
        if out.len() != DH_LEN + payload.len() + TAG_LEN {
            return Err(HandshakeError::Length);
        }
        let e = EphemeralSecret(e_secret);
        let e_pub = e.public();
        ss.mix_hash(&e_pub);
        ss.mix_key(&e_pub);
        let mut shared = e.dh(re);
        ss.mix_key(&shared);
        shared.zeroize();
        out[..DH_LEN].copy_from_slice(&e_pub);
        let n = out.len();
        let body = &mut out[DH_LEN..n - TAG_LEN];
        body.copy_from_slice(payload);
        let tag = ss.encrypt_and_hash(body);
        out[n - TAG_LEN..].copy_from_slice(&tag);
        let (k1, k2) = ss.split();
        Ok(TransportKeys {
            send: k2,
            recv: k1,
            handshake_hash: ss.h,
        })
    }
}

/// An ephemeral X25519 secret (RFC 7748), wiped when dropped. Both the
/// public key and the DH go through the Montgomery ladder (`x25519`), so the
/// image links the ladder alone and none of the Edwards arithmetic a
/// base-point multiplication would otherwise pull in.
struct EphemeralSecret([u8; 32]);

impl EphemeralSecret {
    fn public(&self) -> [u8; DH_LEN] {
        x25519(self.0, X25519_BASEPOINT_BYTES)
    }

    fn dh(&self, peer: [u8; DH_LEN]) -> [u8; DH_LEN] {
        x25519(self.0, peer)
    }
}

impl Drop for EphemeralSecret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// §5.2: the chaining key, the handshake hash and the handshake cipher.
struct SymmetricState {
    ck: [u8; 32],
    h: [u8; 32],
    cipher: CipherState,
}

impl SymmetricState {
    /// `InitializeSymmetric(protocol_name)` then `MixHash(prologue)`.
    fn new(prologue: &[u8]) -> Self {
        // The name is longer than HASHLEN, so it is hashed (§5.2).
        let h: [u8; 32] = Sha256::digest(PROTOCOL_NAME).into();
        let mut ss = SymmetricState {
            ck: h,
            h,
            cipher: CipherState::default(),
        };
        ss.mix_hash(prologue);
        ss
    }

    fn mix_hash(&mut self, data: &[u8]) {
        let mut hasher = Sha256::new();
        hasher.update(self.h);
        hasher.update(data);
        self.h = hasher.finalize().into();
    }

    fn mix_key(&mut self, ikm: &[u8]) {
        let (ck, k) = hkdf2(&self.ck, ikm);
        self.ck = ck;
        self.cipher.initialize_key(k);
    }

    fn mix_key_and_hash(&mut self, ikm: &[u8]) {
        let (ck, mut temp_h, k) = hkdf3(&self.ck, ikm);
        self.ck = ck;
        self.mix_hash(&temp_h);
        temp_h.zeroize();
        self.cipher.initialize_key(k);
    }

    /// `EncryptAndHash` in place; the tag (every lp-link message has a key by
    /// the time it encrypts, see the module docs).
    fn encrypt_and_hash(&mut self, buf: &mut [u8]) -> [u8; TAG_LEN] {
        let h = self.h;
        let tag = self.cipher.encrypt_with_ad(&h, buf).unwrap_or([0; TAG_LEN]);
        let mut hasher = Sha256::new();
        hasher.update(self.h);
        hasher.update(&*buf);
        hasher.update(tag);
        self.h = hasher.finalize().into();
        tag
    }

    /// `DecryptAndHash` in place: `h` absorbs the ciphertext (with its tag)
    /// before decrypting.
    fn decrypt_and_hash(
        &mut self,
        buf: &mut [u8],
        tag: &[u8; TAG_LEN],
    ) -> Result<(), crate::secure_channel::cipher_state::BadTag> {
        let h = self.h;
        let mut hasher = Sha256::new();
        hasher.update(self.h);
        hasher.update(&*buf);
        hasher.update(tag);
        let next_h: [u8; 32] = hasher.finalize().into();
        self.cipher.decrypt_with_ad(&h, buf, Some(tag))?;
        self.h = next_h;
        Ok(())
    }

    /// `Split()`: `(k1, k2)`.
    fn split(&mut self) -> ([u8; 32], [u8; 32]) {
        let out = hkdf2(&self.ck, &[]);
        self.ck.zeroize();
        out
    }
}

impl Clone for SymmetricState {
    fn clone(&self) -> Self {
        SymmetricState {
            ck: self.ck,
            h: self.h,
            cipher: self.cipher.clone(),
        }
    }
}

impl Drop for SymmetricState {
    fn drop(&mut self) {
        self.ck.zeroize();
        self.h.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handshake(
        psk_i: &Psk,
        psk_r: &Psk,
    ) -> Result<(TransportKeys, TransportKeys), HandshakeError> {
        let p = prologue(&KeyId([7; 16]), 0x1234_5678);
        let init = Initiator::new(&p, psk_i, [1; 32]);
        let resp = Responder::new(&p).read_msg1(init.msg1(), psk_r)?;
        let mut msg2 = [0u8; MSG2_LEN];
        let rk = resp.write_msg2([2; 32], &0xCAFE_F00Du32.to_le_bytes(), &mut msg2)?;
        let mut payload = [0u8; MSG2_PAYLOAD_LEN];
        let ik = init.read_msg2(&msg2, &mut payload)?;
        assert_eq!(u32::from_le_bytes(payload), 0xCAFE_F00D);
        Ok((ik, rk))
    }

    #[test]
    fn both_ends_derive_crossed_keys() {
        let psk = Psk::new([9; 32]);
        let (i, r) = handshake(&psk, &psk).unwrap();
        assert_eq!(i.send, r.recv);
        assert_eq!(i.recv, r.send);
        assert_ne!(i.send, i.recv);
        assert_eq!(i.handshake_hash, r.handshake_hash);
    }

    #[test]
    fn a_wrong_psk_fails_msg1_without_panicking() {
        assert_eq!(
            handshake(&Psk::new([9; 32]), &Psk::new([8; 32])).err(),
            Some(HandshakeError::BadTag)
        );
        assert!(handshake(&Psk::ANONYMOUS, &Psk::ANONYMOUS).is_ok());
    }

    #[test]
    fn a_tampered_msg2_fails() {
        let psk = Psk::new([3; 32]);
        let p = prologue(&KeyId::ANONYMOUS, 1);
        for bit in [0usize, 255, 32 * 8 + 1, MSG2_LEN * 8 - 1] {
            let init = Initiator::new(&p, &psk, [4; 32]);
            let resp = Responder::new(&p).read_msg1(init.msg1(), &psk).unwrap();
            let mut msg2 = [0u8; MSG2_LEN];
            resp.write_msg2([5; 32], &[1, 2, 3, 4], &mut msg2).unwrap();
            msg2[bit / 8] ^= 1 << (bit % 8);
            let mut payload = [0u8; 4];
            assert_eq!(
                init.read_msg2(&msg2, &mut payload).err(),
                Some(HandshakeError::BadTag),
                "bit {bit}"
            );
            assert_eq!(payload, [0; 4], "no partial plaintext");
        }
    }

    #[test]
    fn a_different_prologue_fails() {
        let psk = Psk::new([3; 32]);
        let init = Initiator::new(&prologue(&KeyId::ANONYMOUS, 1), &psk, [4; 32]);
        let other = Responder::new(&prologue(&KeyId::ANONYMOUS, 2));
        assert_eq!(
            other.read_msg1(init.msg1(), &psk).err(),
            Some(HandshakeError::BadTag)
        );
    }

    #[test]
    fn wrong_lengths_are_refused() {
        let p = prologue(&KeyId::ANONYMOUS, 1);
        assert_eq!(
            Responder::new(&p)
                .read_msg1(&[0; 47], &Psk::ANONYMOUS)
                .err(),
            Some(HandshakeError::Length)
        );
        let init = Initiator::new(&p, &Psk::ANONYMOUS, [4; 32]);
        assert_eq!(
            init.read_msg2(&[0; MSG2_LEN - 1], &mut [0; 4]).err(),
            Some(HandshakeError::Length)
        );
    }
}
