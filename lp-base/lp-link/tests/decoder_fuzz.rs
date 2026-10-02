//! The receive path under arbitrary input: random bytes in random chunks on a
//! stream, random datagrams, and crafted frames that pass the checksum with
//! arbitrary headers and bodies (SYNs with foreign nonces, data for sequence
//! numbers never sent, fragments that fit no message, ACKs for nothing),
//! interleaved with a real peer's traffic. After every step:
//!
//! - nothing panicked;
//! - the link holds no more RAM than `Link::ram_bound` for its config;
//! - no counter went backwards.
//!
//! Once the input stops, the real peer and the link must be up again and
//! everything the peer sent in the last session must be acknowledged.
//!
//! With features `secure` and `sim` the same runs again with both links secure, the
//! link under test as the responder and as the initiator, and four more kinds
//! of input: replays of the peer's real (sealed) frames, old and in-window;
//! secure SYNs with garbage or truncated msg1/msg2/refusal extensions; forged
//! refusals naming the initiator's nonce; and msg1 floods (the responder
//! keeps one lookup and a bounded event queue). Crafted frames under the
//! session key are sealed frames with garbage tags there. The edge answers
//! lookups from its table and answers a refusal by retrying its key, as a
//! client would.
//!
//! Case count: 256 per framing by default (CI). For a long run,
//! `just link-fuzz 20000` (release).

use lp_link::frame::{self, SYN_LEN};
use lp_link::{
    Arq, CH_CONTROL, CH_LOG, CH_PROTO, CrcKind, Framing, Link, LinkConfig, LinkCounters, LinkEvent,
    LinkState, Micros, SelectiveRepeat,
};
use proptest::prelude::*;

const NONCE_PEER: u32 = 0x1357_9BDF;
const NONCE_LINK: u32 = 0x2468_ACE0;

#[derive(Clone, Debug)]
enum Op {
    /// Raw bytes (stream) or one raw datagram, fed in chunks of `chunk`.
    Garbage { bytes: Vec<u8>, chunk: usize },
    /// A frame with any header and body, checksummed under the session key
    /// (or the SYN key), maybe with one bit flipped afterwards.
    Crafted {
        b0: u8,
        seq: u8,
        ack: u8,
        win: u8,
        body: Vec<u8>,
        syn_key: bool,
        flip: Option<usize>,
    },
    /// The real peer sends a message.
    Send { chan: u8, len: usize },
    /// Up to `n` frames each way between the peer and the link.
    Exchange { n: u8 },
    /// The application reads everything queued.
    Recv,
    /// Time passes.
    Wait { us: u32 },
    /// Secure: one of the peer's earlier frames, fed to the link again.
    #[cfg(all(feature = "secure", feature = "sim"))]
    Replay { pick: usize },
    /// Secure: a SYN with `SECURE` set, content `content` (0–3), and `ext`
    /// after the 12 bytes (any length), naming the link's own nonce as
    /// `your` when `to_link` (a forged refusal, a forged msg2).
    #[cfg(all(feature = "secure", feature = "sim"))]
    SecureSyn {
        content: u8,
        nonce: u32,
        to_link: bool,
        ext: Vec<u8>,
    },
    /// Secure: `n` distinct msg1s at once, with no edge in between.
    #[cfg(all(feature = "secure", feature = "sim"))]
    Msg1Flood { n: u8, seed: u32 },
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => (prop::collection::vec(any::<u8>(), 0..600), 1usize..64)
            .prop_map(|(bytes, chunk)| Op::Garbage { bytes, chunk }),
        3 => (
            any::<u8>(),
            any::<u8>(),
            any::<u8>(),
            any::<u8>(),
            prop_oneof![
                Just(Vec::new()),
                prop::collection::vec(any::<u8>(), SYN_LEN..=SYN_LEN),
                prop::collection::vec(any::<u8>(), 0..300),
            ],
            prop::bool::weighted(0.2),
            prop::option::weighted(0.2, any::<usize>()),
        )
            .prop_map(|(b0, seq, ack, win, body, syn_key, flip)| Op::Crafted {
                b0,
                seq,
                ack,
                win,
                body,
                syn_key,
                flip,
            }),
        2 => (prop_oneof![Just(CH_CONTROL), Just(CH_PROTO), Just(CH_LOG)], 0usize..20_000)
            .prop_map(|(chan, len)| Op::Send { chan, len }),
        3 => (1u8..20).prop_map(|n| Op::Exchange { n }),
        1 => Just(Op::Recv),
        2 => (0u32..300_000).prop_map(|us| Op::Wait { us }),
    ]
}

/// The plain ops, plus the secure ones.
#[cfg(all(feature = "secure", feature = "sim"))]
fn secure_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        8 => op(),
        2 => any::<usize>().prop_map(|pick| Op::Replay { pick }),
        2 => (
            0u8..4,
            any::<u32>(),
            any::<bool>(),
            prop_oneof![
                prop::collection::vec(any::<u8>(), 0..80),
                prop::collection::vec(any::<u8>(), 64..=64),
                prop::collection::vec(any::<u8>(), 52..=52),
                prop::collection::vec(any::<u8>(), 5..=5),
            ],
        )
            .prop_map(|(content, nonce, to_link, ext)| Op::SecureSyn {
                content,
                nonce,
                to_link,
                ext,
            }),
        1 => (1u8..40, any::<u32>()).prop_map(|(n, seed)| Op::Msg1Flood { n, seed }),
    ]
}

fn config() -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(256);
    ProptestConfig {
        cases,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn stream_input_never_breaks_the_link(ops in prop::collection::vec(op(), 1..40)) {
        fuzz::<SelectiveRepeat>(LinkConfig::usb(), &ops, Mode::Plain)?;
    }

    #[test]
    fn datagram_input_never_breaks_the_link(ops in prop::collection::vec(op(), 1..40)) {
        fuzz::<SelectiveRepeat>(LinkConfig::ble(), &ops, Mode::Plain)?;
    }
}

/// Pinned from CI (PR #866, run 36505858435; the fuzz keeps no failure file).
/// A checksum-valid data frame at exactly the sequence number the peer sends
/// next, landing while the link still waits for earlier frames: once the real
/// ones fill the gap the link acknowledges one past anything the peer sent,
/// and the peer used to ignore every ACK after that and resend its flight
/// until the retry limit (about 56 s on `ble()`, 18 s on `usb()`). The window
/// of 0 in the shrunk input plays no part. It must come back at once.
#[test]
fn a_frame_at_the_senders_next_seq_does_not_stall_the_link() {
    for (cfg, seq) in [(LinkConfig::ble(), 27), (LinkConfig::usb(), 24)] {
        let ops = [
            Op::Send { chan: 0, len: 6137 },
            Op::Exchange { n: 10 },
            Op::Exchange { n: 10 },
            Op::Crafted {
                b0: 56,
                seq,
                ack: 0,
                win: 0,
                body: vec![],
                syn_key: false,
                flip: None,
            },
        ];
        // One second of settling, not the fuzz's twenty.
        fuzz_settling::<SelectiveRepeat>(cfg, &ops, Mode::Plain, 200).unwrap();
    }
}

#[cfg(all(feature = "secure", feature = "sim"))]
proptest! {
    #![proptest_config(config())]

    #[test]
    fn a_secure_responder_on_a_stream_never_breaks(ops in prop::collection::vec(secure_op(), 1..40)) {
        fuzz::<SelectiveRepeat>(LinkConfig::usb(), &ops, Mode::SecureResponder)?;
    }

    #[test]
    fn a_secure_initiator_on_a_stream_never_breaks(ops in prop::collection::vec(secure_op(), 1..40)) {
        fuzz::<SelectiveRepeat>(LinkConfig::usb(), &ops, Mode::SecureInitiator)?;
    }

    #[test]
    fn a_secure_responder_on_datagrams_never_breaks(ops in prop::collection::vec(secure_op(), 1..40)) {
        fuzz::<SelectiveRepeat>(LinkConfig::ble().secured(), &ops, Mode::SecureResponder)?;
    }

    /// No ARQ: garbage that passes the CRC resets the session; it must
    /// still come back once the input stops.
    #[test]
    fn a_secure_no_arq_responder_never_breaks(ops in prop::collection::vec(secure_op(), 1..40)) {
        fuzz::<lp_link::NoArq>(LinkConfig::ws(), &ops, Mode::SecureResponder)?;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Plain,
    #[cfg(all(feature = "secure", feature = "sim"))]
    SecureResponder,
    #[cfg(all(feature = "secure", feature = "sim"))]
    SecureInitiator,
}

fn fuzz<A: Arq>(cfg: LinkConfig, ops: &[Op], mode: Mode) -> Result<(), TestCaseError> {
    fuzz_settling::<A>(cfg, ops, mode, 4_000)
}

/// [`fuzz`], allowing `rounds` × 5 ms for the pair to settle rather than the
/// full 20 s — so a pinned regression case fails fast instead of risking a
/// pass by simply outlasting a retry-limit reset it should never need.
fn fuzz_settling<A: Arq>(
    cfg: LinkConfig,
    ops: &[Op],
    mode: Mode,
    rounds: usize,
) -> Result<(), TestCaseError> {
    let mut w = World::<A>::new(cfg, mode);
    let bound = w.bound;
    w.exchange(64);
    for op in ops {
        let before = w.link.counters().clone();
        w.apply(op);
        let after = w.link.counters();
        prop_assert_eq!(
            before.since(after),
            LinkCounters::default(),
            "a counter went backwards after {:?}",
            op
        );
        prop_assert!(
            w.link.ram_bytes() <= bound,
            "{} > {bound} after {:?}",
            w.link.ram_bytes(),
            op
        );
        prop_assert!(w.link.buffered_bytes() + w.link.scratch_bytes() <= bound);
        prop_assert!(w.flood_events <= 4, "{} events queued", w.flood_events);
    }

    // The input stops: the real pair must come back and finish its work.
    for _ in 0..rounds {
        w.now += 5_000;
        w.exchange(64);
        w.recv();
        if w.settled() {
            break;
        }
    }
    prop_assert_eq!(w.link.state(), LinkState::Established);
    prop_assert_eq!(w.peer.state(), LinkState::Established);
    if A::RELIABLE {
        prop_assert!(w.peer.is_idle(), "the peer still holds unacknowledged data");
    }
    prop_assert!(w.link.ram_bytes() <= bound);
    Ok(())
}

struct World<A: Arq> {
    peer: Link<A>,
    link: Link<A>,
    mode: Mode,
    framing: Framing,
    crc: CrcKind,
    now: Micros,
    payload: Vec<u8>,
    bound: usize,
    /// Frames the peer sent the link (for replays).
    captured: Vec<Vec<u8>>,
    /// Events a msg1 flood queued before the edge drained them.
    flood_events: usize,
}

impl<A: Arq> World<A> {
    fn new(cfg: LinkConfig, mode: Mode) -> Self {
        let payload = (0..20_000).map(|i| (i * 31 + 7) as u8).collect();
        let (peer, link, bound) = match mode {
            Mode::Plain => (
                Link::new(cfg.clone(), NONCE_PEER),
                Link::new(cfg.clone(), NONCE_LINK),
                Link::<A>::ram_bound(&cfg),
            ),
            #[cfg(all(feature = "secure", feature = "sim"))]
            Mode::SecureResponder => (
                Link::new_secure(
                    cfg.clone(),
                    NONCE_PEER,
                    secure::host_role(),
                    secure::entropy,
                ),
                Link::new_secure(
                    cfg.clone(),
                    NONCE_LINK,
                    lp_link::secure_channel::SecureRole::Responder,
                    secure::entropy,
                ),
                Link::<A>::ram_bound_secure(&cfg),
            ),
            #[cfg(all(feature = "secure", feature = "sim"))]
            Mode::SecureInitiator => (
                Link::new_secure(
                    cfg.clone(),
                    NONCE_PEER,
                    lp_link::secure_channel::SecureRole::Responder,
                    secure::entropy,
                ),
                Link::new_secure(
                    cfg.clone(),
                    NONCE_LINK,
                    secure::host_role(),
                    secure::entropy,
                ),
                Link::<A>::ram_bound_secure(&cfg),
            ),
        };
        World {
            peer,
            link,
            mode,
            framing: cfg.framing,
            crc: cfg.crc,
            now: 0,
            payload,
            bound,
            captured: Vec::new(),
            flood_events: 0,
        }
    }

    fn apply(&mut self, op: &Op) {
        match op {
            Op::Garbage { bytes, chunk } => match self.framing {
                Framing::Stream => {
                    for c in bytes.chunks(*chunk) {
                        self.link.on_bytes(self.now, c);
                    }
                }
                Framing::Datagram => self.link.on_datagram(self.now, bytes),
            },
            Op::Crafted {
                b0,
                seq,
                ack,
                win,
                body,
                syn_key,
                flip,
            } => {
                let key = if *syn_key { 0 } else { NONCE_PEER ^ NONCE_LINK };
                let mut raw = vec![*b0, *seq, *ack, *win];
                raw.extend_from_slice(body);
                self.feed_crafted(raw, key, *flip);
            }
            Op::Send { chan, len } => {
                let _ = self.peer.send(*chan, &self.payload[..*len]);
            }
            Op::Exchange { n } => self.exchange(*n),
            Op::Recv => self.recv(),
            Op::Wait { us } => self.now += *us as Micros,
            #[cfg(all(feature = "secure", feature = "sim"))]
            Op::Replay { pick } => {
                if !self.captured.is_empty() {
                    let f = self.captured[pick % self.captured.len()].clone();
                    self.feed_link(&f);
                }
            }
            #[cfg(all(feature = "secure", feature = "sim"))]
            Op::SecureSyn {
                content,
                nonce,
                to_link,
                ext,
            } => {
                let your = if *to_link {
                    self.peer.peer_nonce().unwrap_or(0)
                } else {
                    nonce.rotate_left(7)
                };
                let mut raw = vec![3, 0, 0, 0];
                raw.extend_from_slice(&nonce.to_le_bytes());
                raw.extend_from_slice(&your.to_le_bytes());
                raw.push(0x02 | (content << 2));
                raw.extend_from_slice(&256u16.to_le_bytes());
                raw.push(8);
                raw.extend_from_slice(ext);
                self.feed_crafted(raw, 0, None);
            }
            #[cfg(all(feature = "secure", feature = "sim"))]
            Op::Msg1Flood { n, seed } => {
                for i in 0..*n {
                    let nonce = seed.wrapping_add(u32::from(i).wrapping_mul(0x9E37_79B9)) | 1;
                    let mut raw = vec![3, 0, 0, 0];
                    raw.extend_from_slice(&nonce.to_le_bytes());
                    raw.extend_from_slice(&0u32.to_le_bytes());
                    raw.push(0x02 | (1 << 2));
                    raw.extend_from_slice(&256u16.to_le_bytes());
                    raw.push(8);
                    raw.extend(std::iter::repeat_n(i, 16 + 48));
                    self.feed_crafted(raw, 0, None);
                }
                self.flood_events = self.count_link_events();
            }
        }
        // Timers run whenever the edge polls.
        self.edge();
        while let Some(f) = self.link.poll_transmit(self.now) {
            let f = f.to_vec();
            self.feed_peer(&f);
        }
    }

    /// `raw` (header and body), checksummed under `key`, framed, maybe with
    /// one bit flipped, fed to the link.
    fn feed_crafted(&mut self, mut raw: Vec<u8>, key: u32, flip: Option<usize>) {
        let sum = self.crc.compute(key, &raw).to_le_bytes();
        raw.extend_from_slice(&sum[..self.crc.len()]);
        let mut wire = match self.framing {
            Framing::Stream => {
                let mut out = Vec::new();
                frame::wrap_stream(&raw, &mut out);
                out
            }
            Framing::Datagram => raw,
        };
        if let Some(bit) = flip {
            let bit = bit % (wire.len() * 8);
            wire[bit / 8] ^= 1 << (bit % 8);
        }
        self.feed_link(&wire);
    }

    /// Up to `n` frames each way, peer first.
    fn exchange(&mut self, n: u8) {
        for _ in 0..n {
            self.edge();
            let mut moved = false;
            if let Some(f) = self.peer.poll_transmit(self.now) {
                let f = f.to_vec();
                if self.mode != Mode::Plain && self.captured.len() < 256 {
                    self.captured.push(f.clone());
                }
                self.feed_link(&f);
                moved = true;
            }
            if let Some(f) = self.link.poll_transmit(self.now) {
                let f = f.to_vec();
                self.feed_peer(&f);
                moved = true;
            }
            if !moved {
                break;
            }
        }
        while self.peer.recv().is_some() {}
    }

    fn recv(&mut self) {
        while let Some(ev) = self.link.recv() {
            if let LinkEvent::Message { data, .. } = ev {
                assert!(data.len() <= self.payload.len());
            }
        }
    }

    fn settled(&self) -> bool {
        self.link.state() == LinkState::Established
            && self.peer.state() == LinkState::Established
            && (!A::RELIABLE || self.peer.is_idle())
    }

    fn feed_link(&mut self, wire: &[u8]) {
        match self.framing {
            Framing::Stream => self.link.on_bytes(self.now, wire),
            Framing::Datagram => self.link.on_datagram(self.now, wire),
        }
    }

    fn feed_peer(&mut self, wire: &[u8]) {
        match self.framing {
            Framing::Stream => self.peer.on_bytes(self.now, wire),
            Framing::Datagram => self.peer.on_datagram(self.now, wire),
        }
    }

    /// Both ends' edges: answer lookups, retry after a refusal.
    fn edge(&mut self) {
        #[cfg(all(feature = "secure", feature = "sim"))]
        {
            secure::edge(&mut self.peer);
            secure::edge(&mut self.link);
        }
    }

    #[cfg(all(feature = "secure", feature = "sim"))]
    fn count_link_events(&mut self) -> usize {
        let mut n = 0;
        while self.link.poll_secure_event().is_some() {
            n += 1;
        }
        n
    }
}

#[cfg(all(feature = "secure", feature = "sim"))]
mod secure {
    use lp_link::secure_channel::{KeyId, Psk, RefusalReason, SecureEvent, SecureRole};
    use lp_link::{Arq, Link};

    const KEY: KeyId = KeyId([0x42; 16]);

    fn psk() -> Psk {
        Psk::new([0x24; 32])
    }

    pub fn host_role() -> SecureRole {
        SecureRole::Initiator {
            key_id: KEY,
            psk: psk(),
        }
    }

    pub fn entropy(buf: &mut [u8]) {
        lp_link::sim::sim_entropy::fill(buf);
    }

    /// A device edge (answers lookups from a one-key table) and a client
    /// edge (retries its key after a refusal), whichever the link is.
    pub fn edge<A: Arq>(link: &mut Link<A>) {
        while let Some(ev) = link.poll_secure_event() {
            match ev {
                SecureEvent::KeyLookup { key_id } if key_id == KEY => {
                    link.provide_keys(key_id, &[psk()]);
                }
                SecureEvent::KeyLookup { key_id } => {
                    link.refuse(key_id, RefusalReason::UnknownKey, 0);
                }
                SecureEvent::Refused { .. } => link.retry_with(KEY, psk()),
                SecureEvent::WrongKey { .. } | SecureEvent::PeerNotSecure => {}
            }
        }
    }
}
