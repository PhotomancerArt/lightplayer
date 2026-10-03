//! A secure link's frames after the handshake (feature `secure`): every one
//! but a SYN is sealed.
//!
//! ```text
//! header[4] ‖ ctr[4, LE] ‖ ciphertext ‖ tag[16] ‖ crc[2|4]
//! ```
//!
//! ChaCha20-Poly1305 under this direction's key from the handshake's
//! `Split()`, nonce = `0^32 ‖ LE64(ctr)` (Noise's own encoding), associated
//! data = the four header bytes, so a frame's kind, channel, sequence, ACK
//! and window are authenticated with its payload (they stay readable).
//!
//! - **One counter per direction**, from 0 each session, taken by every
//!   transmission: data, datagrams, ACKs, and every **retransmission**, which
//!   is re-sealed from the plaintext the transmit window keeps. A resend
//!   carries a different ACK and window, so reusing its old ciphertext would
//!   reuse a nonce under different associated data.
//! - **The CRC stays** (keyed by both nonces, as on a plain link) and is
//!   checked first: a CRC failure is line damage (`bad_frames`); a frame that
//!   passes it and fails its tag is a forgery or a bug (`seal_failures`).
//! - **Replay.** ARQ links keep a 64-frame window: a counter seen (or too
//!   old to tell) is dropped and counted (`replays`), a bad tag dropped and
//!   counted; ARQ resends what was lost. A no-ARQ link (a WebSocket, the
//!   relay) takes only the next counter: nothing resends there, so a gap is
//!   lost data or a relay dropping frames, and the link **resets**
//!   (`counter_gaps`), as it does on a bad tag. A replayed old counter is
//!   dropped.
//! - `max_payload` keeps meaning plaintext: a sealed frame is
//!   [`SEAL_OVERHEAD`] bytes longer ([`LinkConfig::secured`](crate::LinkConfig::secured)
//!   for a transport with a hard frame size). Sealing happens in place in
//!   the frame scratch, so a frame allocates nothing.

use core::mem;

use super::Link;
use crate::Micros;
use crate::arq::Arq;
use crate::frame::{HEADER_LEN, Header};
use crate::link_config::SEAL_OVERHEAD;
use crate::link_event::ResetReason;
use crate::secure_channel::ReplayVerdict;
use crate::secure_channel::cipher_state::TAG_LEN;

const CTR_LEN: usize = 4;

impl<A: Arq> Link<A> {
    /// Bytes sealing adds to a frame on this link (0 on a plain link).
    pub(super) fn seal_overhead(&self) -> usize {
        if self.secure.is_some() {
            SEAL_OVERHEAD
        } else {
            0
        }
    }

    /// `raw` holds a plain frame (`header ‖ body ‖ crc`); seal it in place
    /// into `header ‖ ctr ‖ ciphertext ‖ tag ‖ crc` under the next counter.
    /// Only SYNs are sent before a secure link has keys, and they never come
    /// here.
    pub(super) fn seal_raw(&mut self) {
        let key = self.key();
        let crc = self.cfg.crc;
        let Some(sec) = self.secure.as_mut() else {
            return;
        };
        let Some(keys) = sec.keys.as_ref() else {
            debug_assert!(false, "a frame sealed before the handshake split");
            self.raw.clear();
            return;
        };
        let ctr = sec.send_ctr;
        sec.send_ctr = sec.send_ctr.saturating_add(1);
        let body_len = self.raw.len() - HEADER_LEN - crc.len();
        // header ‖ body → header ‖ ctr ‖ body (within the reserved capacity).
        self.raw.truncate(HEADER_LEN + body_len);
        self.raw.resize(HEADER_LEN + CTR_LEN + body_len, 0);
        self.raw
            .copy_within(HEADER_LEN..HEADER_LEN + body_len, HEADER_LEN + CTR_LEN);
        self.raw[HEADER_LEN..HEADER_LEN + CTR_LEN].copy_from_slice(&ctr.to_le_bytes());
        let (head, rest) = self.raw.split_at_mut(HEADER_LEN);
        let tag = keys.send.seal(u64::from(ctr), head, &mut rest[CTR_LEN..]);
        self.raw.extend_from_slice(&tag);
        let sum = crc.compute(key, &self.raw).to_le_bytes();
        self.raw.extend_from_slice(&sum[..crc.len()]);
    }

    /// A frame that passed its CRC on a secure link: `body` is `ctr ‖
    /// ciphertext ‖ tag`. Replay check, open, then the plain handling of the
    /// plaintext. `true`: it was a whole frame (the deframer's resync rule),
    /// whatever became of it.
    pub(super) fn on_sealed_frame(
        &mut self,
        now: Micros,
        hdr: &Header,
        raw: &[u8],
        body: &[u8],
    ) -> bool {
        let Some(sec) = self.secure.as_mut() else {
            return true;
        };
        let Some(keys) = sec.keys.as_ref() else {
            // An initiator before msg2, or a responder with no handshake: a
            // frame of no session we hold.
            self.counters.dropped_unsynced += 1;
            return true;
        };
        if body.len() < SEAL_OVERHEAD {
            self.counters.seal_failures += 1;
            self.seal_failed(now);
            return true;
        }
        let ctr = u32::from_le_bytes([body[0], body[1], body[2], body[3]]);
        let verdict = sec.replay.check(ctr);
        if matches!(verdict, ReplayVerdict::Replay | ReplayVerdict::TooOld) {
            self.counters.replays += 1;
            return true;
        }
        let ct = &body[CTR_LEN..body.len() - TAG_LEN];
        let mut tag = [0u8; TAG_LEN];
        tag.copy_from_slice(&body[body.len() - TAG_LEN..]);
        // Open in the transmit scratch (idle between `poll_transmit` calls).
        let mut plain = mem::take(&mut self.raw);
        plain.clear();
        plain.extend_from_slice(ct);
        if keys
            .recv
            .open(u64::from(ctr), &raw[..HEADER_LEN], &mut plain, &tag)
            .is_err()
        {
            self.raw = plain;
            self.counters.seal_failures += 1;
            self.seal_failed(now);
            return true;
        }
        if verdict == ReplayVerdict::Gap {
            // Genuine (it opened), but frames before it never came: on a
            // no-ARQ link that is lost data, and nothing will resend it.
            self.raw = plain;
            self.counters.counter_gaps += 1;
            self.reset(now, ResetReason::ProtocolError);
            return true;
        }
        sec.replay.mark(ctr);
        if let Some(h) = sec.half_open.take() {
            // The responder's key confirmation: the initiator holds the
            // session's keys. `on_verified` brings the link up.
            sec.auth = Some(h.auth);
        }
        let ok = self.on_verified(now, hdr, &plain);
        self.raw = plain;
        ok
    }

    /// A frame passed its CRC and failed its tag: dropped on an ARQ link
    /// (ARQ resends the genuine one); on a no-ARQ link nothing would, so the
    /// link fails closed and resets.
    fn seal_failed(&mut self, now: Micros) {
        if !A::RELIABLE {
            self.reset(now, ResetReason::ProtocolError);
        }
    }
}
