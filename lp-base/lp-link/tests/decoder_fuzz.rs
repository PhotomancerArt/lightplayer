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
//! Case count: 256 per framing by default (CI). For a long run,
//! `just link-fuzz 20000` (release).

use lp_link::frame::{self, SYN_LEN};
use lp_link::{
    CH_CONTROL, CH_LOG, CH_PROTO, CrcKind, Framing, Link, LinkConfig, LinkCounters, LinkEvent,
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
        fuzz(LinkConfig::usb(), &ops)?;
    }

    #[test]
    fn datagram_input_never_breaks_the_link(ops in prop::collection::vec(op(), 1..40)) {
        fuzz(LinkConfig::ble(), &ops)?;
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
        fuzz_settling(cfg, &ops, 200).unwrap();
    }
}

fn fuzz(cfg: LinkConfig, ops: &[Op]) -> Result<(), TestCaseError> {
    fuzz_settling(cfg, ops, 4_000)
}

/// [`fuzz`], allowing `rounds` × 5 ms for the pair to settle.
fn fuzz_settling(cfg: LinkConfig, ops: &[Op], rounds: usize) -> Result<(), TestCaseError> {
    let bound = Link::<SelectiveRepeat>::ram_bound(&cfg);
    let mut w = World {
        peer: Link::new(cfg.clone(), NONCE_PEER),
        link: Link::new(cfg.clone(), NONCE_LINK),
        framing: cfg.framing,
        crc: cfg.crc,
        now: 0,
        payload: (0..20_000).map(|i| (i * 31 + 7) as u8).collect(),
    };
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
    prop_assert!(w.peer.is_idle(), "the peer still holds unacknowledged data");
    prop_assert!(w.link.ram_bytes() <= bound);
    Ok(())
}

struct World {
    peer: Link<SelectiveRepeat>,
    link: Link<SelectiveRepeat>,
    framing: Framing,
    crc: CrcKind,
    now: Micros,
    payload: Vec<u8>,
}

impl World {
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
            Op::Send { chan, len } => {
                let _ = self.peer.send(*chan, &self.payload[..*len]);
            }
            Op::Exchange { n } => self.exchange(*n),
            Op::Recv => self.recv(),
            Op::Wait { us } => self.now += *us as Micros,
        }
        // Timers run whenever the edge polls.
        while let Some(f) = self.link.poll_transmit(self.now) {
            let f = f.to_vec();
            self.feed_peer(&f);
        }
    }

    /// Up to `n` frames each way, peer first.
    fn exchange(&mut self, n: u8) {
        for _ in 0..n {
            let mut moved = false;
            if let Some(f) = self.peer.poll_transmit(self.now) {
                let f = f.to_vec();
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
            && self.peer.is_idle()
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
}
