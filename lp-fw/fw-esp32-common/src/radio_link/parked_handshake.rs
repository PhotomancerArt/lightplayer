//! A newcomer's first frame, held while the network slot is busy (Wi-Fi
//! relay plan D2, RD9): the one network session either stays with whoever
//! holds it, or passes to a handshake that proves the **same key**.
//!
//! The board holds one network session (a secure lp-link costs ~14 KB of a
//! heap whose read gate has ~2 KB to spare), shared by the LAN endpoint and
//! the relay. When the slot is held and another connection arrives on the
//! other path, its edge does not open a second session: it parks the
//! newcomer's first frame here — the initiator's SYN, which carries Noise's
//! msg1 and names its key id in the clear — and the mux decides:
//!
//! - **another key id** (or an anonymous one, or a holder whose session is
//!   not up): busy, at once, with no lookup and nothing charged to the login
//!   backoff;
//! - **the holder's key id**: the server looks the key up through the same
//!   `lpc-access` path every handshake takes, and msg1 is checked against it
//!   ([`Msg1::verifies_with`]: Noise's `read_msg1`, no Diffie-Hellman). Only
//!   a msg1 that verifies takes the slot: the holder is closed, and the
//!   newcomer's edge opens its session with the parked frame as its first.
//!   A wrong key is a failed guess, charged to the backoff like any other.
//!
//! Verification comes before eviction, so a stranger who knows a key id
//! (they are sent in the clear) can never knock a session down. This is
//! what Studio's automatic LAN upgrade (M8) rides: the same browser opens the
//! LAN with the key it holds on the relay, and its new session replaces its
//! old one.
//!
//! The buffer is the slot's own, made with it ([`PARKED_FRAME_MAX`] bytes),
//! so a challenge allocates nothing.

use lp_link::frame::secure_syn::{SecureSyn, SynExt};
use lp_link::frame::{FrameKind, Header, verify};
use lp_link::secure_channel::{KeyId, MSG1_LEN, Psk, Responder, prologue};
use lpc_shared::transport::LinkId;

use super::lan_link_config::lan_link_config;
use super::slot_edge::SlotEdge;

/// The longest frame a challenge parks: a msg1 SYN is 4 header bytes, a
/// 76-byte body and a 4-byte checksum (84); anything longer is not one.
pub const PARKED_FRAME_MAX: usize = 96;

/// What the mux decided about a parked handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengeVerdict {
    /// The newcomer proved the holder's key: the slot is its to open.
    TakeOver,
    /// Busy: the newcomer is turned away (its edge says so in its own
    /// words: WebSocket 1013 on the LAN, a `Busy` route close on the relay).
    Busy,
}

/// Where a parked handshake stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParkState {
    Empty,
    /// Parked; the mux has not decided.
    Waiting,
    /// The mux granted it: the slot is reserved for its edge's open.
    Granted,
}

/// The slot's one parked handshake (see the module doc).
pub struct ParkedHandshake {
    state: ParkState,
    id: LinkId,
    edge: SlotEdge,
    len: u8,
    frame: [u8; PARKED_FRAME_MAX],
}

/// Why a frame was not parked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParkRefused {
    /// Another newcomer is already waiting.
    Occupied,
    /// The frame is longer than any msg1 SYN.
    TooLong,
}

impl ParkedHandshake {
    /// An empty one.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: ParkState::Empty,
            id: LinkId::PRIMARY,
            edge: SlotEdge::Local,
            len: 0,
            frame: [0; PARKED_FRAME_MAX],
        }
    }

    /// Park `frame` as `id`'s first, from `edge`.
    pub fn park(&mut self, id: LinkId, edge: SlotEdge, frame: &[u8]) -> Result<(), ParkRefused> {
        if self.state != ParkState::Empty {
            return Err(ParkRefused::Occupied);
        }
        if frame.len() > PARKED_FRAME_MAX {
            return Err(ParkRefused::TooLong);
        }
        self.frame[..frame.len()].copy_from_slice(frame);
        self.len = frame.len() as u8;
        self.id = id;
        self.edge = edge;
        self.state = ParkState::Waiting;
        Ok(())
    }

    /// Whether anything is parked (waiting or granted): the slot is
    /// reserved while it is.
    #[must_use]
    pub fn is_occupied(&self) -> bool {
        self.state != ParkState::Empty
    }

    /// `id`'s parked msg1, while the mux has not decided.
    #[must_use]
    pub fn waiting_msg1(&self, id: LinkId) -> Option<Option<Msg1>> {
        (self.state == ParkState::Waiting && self.id == id)
            .then(|| Msg1::parse(&self.frame[..usize::from(self.len)]))
    }

    /// The mux granted `id` the slot; `false` if `id` is not waiting.
    pub fn grant(&mut self, id: LinkId) -> bool {
        if self.state == ParkState::Waiting && self.id == id {
            self.state = ParkState::Granted;
            true
        } else {
            false
        }
    }

    /// The edge `id` came from, if it was granted.
    #[must_use]
    pub fn granted_edge(&self, id: LinkId) -> Option<SlotEdge> {
        (self.state == ParkState::Granted && self.id == id).then_some(self.edge)
    }

    /// `id`'s frame, if it was granted, and the edge it came from; the
    /// handshake is no longer parked.
    pub fn take_granted(&mut self, id: LinkId) -> Option<(SlotEdge, &[u8])> {
        if self.state != ParkState::Granted || self.id != id {
            return None;
        }
        self.state = ParkState::Empty;
        Some((self.edge, &self.frame[..usize::from(self.len)]))
    }

    /// Forget `id`'s handshake (refused, or its edge gave up); whether it
    /// was parked.
    pub fn clear(&mut self, id: LinkId) -> bool {
        if self.state != ParkState::Empty && self.id == id {
            self.state = ParkState::Empty;
            true
        } else {
            false
        }
    }
}

impl Default for ParkedHandshake {
    fn default() -> Self {
        Self::new()
    }
}

/// An initiator's msg1, out of its SYN: the key id it names (in the clear),
/// its lp-link nonce (in the Noise prologue), and the Noise message.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Msg1 {
    pub key_id: KeyId,
    pub nonce: u32,
    msg: [u8; MSG1_LEN],
}

impl Msg1 {
    /// Parse one whole lp-link frame as a network link's (the LAN preset's
    /// checksum; a SYN is checksummed under key 0): `None` unless it is a
    /// secure SYN carrying msg1.
    #[must_use]
    pub fn parse(raw: &[u8]) -> Option<Self> {
        let header = Header::parse(raw)?;
        if header.kind != FrameKind::Syn {
            return None;
        }
        let body = verify(lan_link_config().crc, 0, raw)?;
        let syn = SecureSyn::parse(body)?;
        match syn.ext {
            SynExt::Msg1 { key_id, msg } if syn.secure => Some(Self {
                key_id: KeyId(key_id),
                nonce: syn.base.nonce,
                msg,
            }),
            _ => None,
        }
    }

    /// Whether msg1 verifies under `psk` (Noise's responder `read_msg1`:
    /// HKDFs and one AEAD tag, no DH).
    #[must_use]
    pub fn verifies_with(&self, psk: &[u8; 32]) -> bool {
        Responder::new(&prologue(&self.key_id, self.nonce))
            .read_msg1(&self.msg, &Psk::new(*psk))
            .is_ok()
    }
}

/// The key id is public; the message is not interesting to print.
impl core::fmt::Debug for Msg1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Msg1").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use lp_link::secure_channel::SecureRole;
    use lp_link::{Link, LinkConfig, SelectiveRepeat};

    extern crate std;

    #[test]
    fn an_initiators_first_frame_is_its_msg1_and_verifies_only_under_its_key() {
        let key = KeyId([7; 16]);
        let (raw, nonce) = first_frame(key, [9; 32]);
        let msg1 = Msg1::parse(&raw).expect("a secure SYN with msg1");
        assert_eq!(msg1.key_id, key);
        assert_eq!(msg1.nonce, nonce);
        assert!(msg1.verifies_with(&[9; 32]));
        assert!(!msg1.verifies_with(&[8; 32]));
        let mut damaged = raw.clone();
        damaged[20] ^= 1;
        assert!(Msg1::parse(&damaged).is_none(), "the checksum fails");
        assert!(raw.len() <= PARKED_FRAME_MAX, "{} B", raw.len());
    }

    #[test]
    fn one_handshake_parks_at_a_time_and_only_its_grant_releases_it() {
        let mut parked = ParkedHandshake::new();
        let (raw, _) = first_frame(KeyId([1; 16]), [2; 32]);
        let a = LinkId::new(5);
        let b = LinkId::new(6);
        parked.park(a, SlotEdge::Relay, &raw).unwrap();
        assert_eq!(
            parked.park(b, SlotEdge::Local, &raw),
            Err(ParkRefused::Occupied)
        );
        assert!(parked.waiting_msg1(b).is_none());
        assert!(parked.waiting_msg1(a).unwrap().is_some());
        assert!(parked.take_granted(a).is_none(), "not granted yet");
        assert!(parked.grant(a));
        assert!(parked.waiting_msg1(a).is_none(), "decided");
        assert!(parked.is_occupied(), "granted still reserves the slot");
        let (edge, frame) = parked.take_granted(a).unwrap();
        assert_eq!((edge, frame), (SlotEdge::Relay, raw.as_slice()));
        assert!(!parked.is_occupied());
        assert_eq!(
            parked.park(b, SlotEdge::Local, &[0; PARKED_FRAME_MAX + 1]),
            Err(ParkRefused::TooLong)
        );
        parked.park(b, SlotEdge::Local, &raw).unwrap();
        assert!(!parked.clear(a));
        assert!(parked.clear(b));
    }

    /// The first frame a secure initiator on the LAN preset sends.
    fn first_frame(key: KeyId, psk: [u8; 32]) -> (std::vec::Vec<u8>, u32) {
        fn entropy(buf: &mut [u8]) {
            buf.fill(0x42);
        }
        let nonce = 0x1234_5678;
        let mut link = Link::<SelectiveRepeat>::new_secure(
            LinkConfig::ws(),
            nonce,
            SecureRole::Initiator {
                key_id: key,
                psk: Psk::new(psk),
            },
            entropy,
        );
        let frame = link.poll_transmit(0).expect("a SYN at once").to_vec();
        (frame, nonce)
    }
}
