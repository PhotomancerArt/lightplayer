//! The secure handshake inside lp-link's SYN, leg by leg, and sealed frames
//! between two links wired back to back (no simulator: every frame is in
//! the test's hands, so a test can drop, replay or forge one).
//!
//! The host is the initiator (it holds a key); the board is the responder,
//! and this file plays its edge: it answers `KeyLookup` from a key table,
//! now, later, or never.

use lp_link::secure_channel::{KeyId, Psk, RefusalReason, SecureEvent, SecureRole, SessionAuth};
use lp_link::sim::sim_entropy;
use lp_link::{
    Arq, CH_PROTO, Framing, Link, LinkConfig, LinkEvent, LinkState, Micros, NoArq, ResetReason,
    SelectiveRepeat,
};

const HOST_NONCE: u32 = 0x1111_1111;
const BOARD_NONCE: u32 = 0x2222_2222;

fn key(n: u8) -> (KeyId, Psk) {
    (
        KeyId([n; 16]),
        Psk::new([n.wrapping_mul(7).wrapping_add(1); 32]),
    )
}

// ---- Handshake legs ---------------------------------------------------------

#[test]
fn initiator_first_comes_up_with_crossed_keys() {
    let mut p = Pair::<SelectiveRepeat>::usb(1);
    p.run_until_up();
    let (hs, hr) = p.host.secure_keys_for_test().unwrap();
    let (bs, br) = p.board.secure_keys_for_test().unwrap();
    assert_eq!((hs, hr), (br, bs), "crossed: host send = board recv");
    assert_ne!(hs, hr);
    assert_eq!(
        p.board.session_auth(),
        Some(SessionAuth {
            key_id: key(1).0,
            candidate: 0
        })
    );
    assert_eq!(p.host.counters().handshakes, 1);
    assert_eq!(p.board.counters().handshakes, 1);
    assert_eq!(p.lookups, vec![key(1).0]);
    p.both_ways_carry_messages();
}

#[test]
fn responder_first_comes_up() {
    let mut p = Pair::<SelectiveRepeat>::usb(1);
    // The board speaks first (presence), then the host.
    let f = p.board.poll_transmit(p.now).unwrap().to_vec();
    assert!(!f.is_empty());
    p.feed_host(&f);
    p.run_until_up();
    p.both_ways_carry_messages();
}

#[test]
fn a_simultaneous_start_comes_up() {
    let mut p = Pair::<NoArq>::ws(1);
    let a = p.host.poll_transmit(0).unwrap().to_vec();
    let b = p.board.poll_transmit(0).unwrap().to_vec();
    p.feed_board(&a);
    p.feed_host(&b);
    p.run_until_up();
    p.both_ways_carry_messages();
}

#[test]
fn a_lost_msg1_is_sent_again() {
    let mut p = Pair::<SelectiveRepeat>::usb(1);
    p.drop_host_frames = 1;
    p.run_until_up();
    // The board's presence SYN draws the resend at once (no wait for the
    // SYN timer), and the resend is looked up once.
    assert!(p.host.counters().frames_tx >= 2);
    assert_eq!(p.lookups.len(), 1);
}

#[test]
fn a_lost_msg2_is_answered_again_with_the_same_bytes() {
    let mut p = Pair::<NoArq>::ws(1);
    // msg1 → board; the lookup is answered; msg2 is lost.
    let msg1 = p.host.poll_transmit(0).unwrap().to_vec();
    p.feed_board(&msg1);
    p.answer_lookups();
    let msg2 = p.board.poll_transmit(0).unwrap().to_vec();
    assert!(p.board.is_half_open_for_test());
    // The host's next SYN is msg1 again, byte for byte.
    p.now = LinkConfig::ws().syn_interval;
    let again = p.host.poll_transmit(p.now).unwrap().to_vec();
    assert_eq!(
        again, msg1,
        "one ephemeral per session: msg1 resends identically"
    );
    p.feed_board(&again);
    assert!(p.lookups.len() == 1, "a resend is not looked up twice");
    let msg2_again = p.board.poll_transmit(p.now).unwrap().to_vec();
    assert_eq!(msg2_again, msg2);
    p.feed_host(&msg2_again);
    assert_eq!(p.host.state(), LinkState::Established);
    p.run_until_up();
}

#[test]
fn a_lost_confirmation_is_healed_by_the_next_msg2() {
    let mut p = Pair::<SelectiveRepeat>::usb(1);
    let msg1 = p.host.poll_transmit(0).unwrap().to_vec();
    p.feed_board(&msg1);
    p.answer_lookups();
    let msg2 = p.board.poll_transmit(0).unwrap().to_vec();
    p.feed_host(&msg2);
    assert_eq!(p.host.state(), LinkState::Established);
    // The host's sealed ACK (key confirmation) is lost.
    let _lost = p.host.poll_transmit(0).unwrap().to_vec();
    assert_eq!(p.board.state(), LinkState::Connecting);
    p.run_until_up();
    assert_eq!(p.board.counters().ups, 1);
}

#[test]
fn a_late_lookup_answer_still_comes_up() {
    let mut p = Pair::<SelectiveRepeat>::usb(1);
    p.answer_after = Some(500_000);
    p.run_until_up();
    assert!(p.now >= 500_000);
}

#[test]
fn a_lookup_never_answered_is_refused_busy() {
    let mut p = Pair::<SelectiveRepeat>::usb(1);
    p.answer_after = None;
    p.run_for(3_000_000);
    assert_eq!(
        p.host_events,
        vec![SecureEvent::Refused {
            reason: RefusalReason::Busy,
            retry_after_ms: 0
        }]
    );
    assert_ne!(p.host.state(), LinkState::Established);
    assert_eq!(p.board.counters().handshake_refusals, 1);
}

#[test]
fn an_unknown_key_is_refused_and_costs_nothing() {
    let mut p = Pair::<SelectiveRepeat>::usb(9);
    p.run_for(1_000_000);
    assert_eq!(
        p.host_events,
        vec![SecureEvent::Refused {
            reason: RefusalReason::UnknownKey,
            retry_after_ms: 0
        }]
    );
    assert!(
        p.board_events.is_empty(),
        "no WrongKey: {:?}",
        p.board_events
    );
    // Refused: the host stops sending SYNs.
    let sent = p.host.counters().frames_tx;
    p.run_for(2_000_000);
    assert_eq!(p.host.counters().frames_tx, sent);
}

#[test]
fn backoff_is_refused_with_its_wait() {
    let mut p = Pair::<SelectiveRepeat>::usb(1);
    p.backoff_ms = Some(8_000);
    p.run_for(1_000_000);
    assert_eq!(
        p.host_events,
        vec![SecureEvent::Refused {
            reason: RefusalReason::Backoff,
            retry_after_ms: 8_000
        }]
    );
}

#[test]
fn a_wrong_key_is_refused_and_reported_once() {
    let mut p = Pair::<SelectiveRepeat>::usb(1);
    // The board knows key 1's id, but under another PSK.
    p.table = vec![(key(1).0, vec![Psk::new([0xEE; 32])])];
    p.run_for(1_000_000);
    assert_eq!(
        p.host_events,
        vec![SecureEvent::Refused {
            reason: RefusalReason::WrongKey,
            retry_after_ms: 0
        }]
    );
    assert_eq!(
        p.board_events,
        vec![SecureEvent::WrongKey { key_id: key(1).0 }]
    );
    assert_ne!(p.host.state(), LinkState::Established);
    assert_ne!(p.board.state(), LinkState::Established);
}

#[test]
fn a_lost_refusal_is_repeated_without_a_second_lookup() {
    let mut p = Pair::<NoArq>::ws(1);
    p.table = vec![(key(1).0, vec![Psk::new([0xEE; 32])])];
    let msg1 = p.host.poll_transmit(0).unwrap().to_vec();
    p.feed_board(&msg1);
    p.answer_lookups();
    let _lost_refusal = p.board.poll_transmit(0).unwrap().to_vec();
    p.feed_board(&msg1);
    p.answer_lookups();
    let refusal = p.board.poll_transmit(0).unwrap().to_vec();
    p.feed_host(&refusal);
    p.drain();
    assert_eq!(p.lookups.len(), 1);
    assert_eq!(
        p.board_events.len(),
        1,
        "charged once: {:?}",
        p.board_events
    );
    assert!(matches!(p.host_events[..], [SecureEvent::Refused { .. }]));
}

#[test]
fn the_second_candidate_can_match() {
    let mut p = Pair::<SelectiveRepeat>::usb(1);
    p.table = vec![(key(1).0, vec![Psk::new([0xEE; 32]), key(1).1])];
    p.run_until_up();
    assert_eq!(p.board.session_auth().map(|a| a.candidate), Some(1));
}

#[test]
fn retry_with_another_key_after_a_refusal() {
    let mut p = Pair::<SelectiveRepeat>::usb(9);
    p.run_for(500_000);
    assert!(matches!(p.host_events[..], [SecureEvent::Refused { .. }]));
    let (k, psk) = key(1);
    p.host.retry_with(k, psk);
    p.run_until_up();
    assert_eq!(p.board.session_auth().map(|a| a.key_id), Some(key(1).0));
}

#[test]
fn the_anonymous_key_comes_up() {
    let mut p = Pair::<NoArq>::ws(0);
    p.table = vec![(KeyId::ANONYMOUS, vec![Psk::ANONYMOUS])];
    p.host = Link::new_secure(
        LinkConfig::ws(),
        HOST_NONCE,
        SecureRole::Initiator {
            key_id: KeyId::ANONYMOUS,
            psk: Psk::ANONYMOUS,
        },
        sim_entropy::fill,
    );
    p.run_until_up();
    assert_eq!(
        p.board.session_auth().map(|a| a.key_id),
        Some(KeyId::ANONYMOUS)
    );
}

// ---- Who may reset a session -----------------------------------------------

#[test]
fn a_plain_syn_never_resets_a_secure_responder() {
    let mut p = Pair::<NoArq>::ws(1);
    p.run_until_up();
    let mut plain = Link::<NoArq>::new(LinkConfig::ws(), 0x7777_7777);
    let f = plain.poll_transmit(p.now).unwrap().to_vec();
    p.feed_board(&f);
    assert_eq!(p.board.state(), LinkState::Established);
    assert_eq!(p.board.counters().resets, 0);
    assert_eq!(p.board.counters().secure_syn_ignored, 1);
    p.both_ways_carry_messages();
}

#[test]
fn a_msg1_that_fails_never_resets_a_secure_responder() {
    let mut p = Pair::<NoArq>::ws(1);
    p.run_until_up();
    // Another initiator with the right key id and the wrong PSK.
    let mut forger = Link::<NoArq>::new_secure(
        LinkConfig::ws(),
        0x7777_7777,
        SecureRole::Initiator {
            key_id: key(1).0,
            psk: Psk::new([0xAB; 32]),
        },
        sim_entropy::fill,
    );
    let f = forger.poll_transmit(p.now).unwrap().to_vec();
    p.feed_board(&f);
    p.answer_lookups();
    p.drain();
    assert_eq!(p.board.state(), LinkState::Established);
    assert_eq!(p.board.counters().resets, 0);
    assert_eq!(
        p.board_events,
        vec![SecureEvent::WrongKey { key_id: key(1).0 }]
    );
    p.both_ways_carry_messages();
}

/// NNpsk0's first message is replayable: an on-path attacker holding an
/// old, genuine msg1 can make the device drop its session (accepted as
/// on-path denial of service, plan SQ8). It cannot complete the handshake.
#[test]
fn a_replayed_old_msg1_verifies_and_resets_but_goes_nowhere() {
    let mut p = Pair::<NoArq>::ws(1);
    let old_msg1 = p.host.poll_transmit(0).unwrap().to_vec();
    p.feed_board(&old_msg1);
    p.run_until_up();
    // The host restarts: a new session.
    p.host.restart(p.now);
    p.run_until_up();
    let resets_before = p.board.counters().resets;
    p.feed_board(&old_msg1);
    p.answer_lookups();
    p.drain();
    assert_eq!(p.board.counters().resets, resets_before + 1);
    assert_eq!(p.board.state(), LinkState::Connecting);
    // The real host comes back on its own.
    p.run_until_up();
}

/// Found by the decoder fuzzer: an old msg1 replayed while the board is
/// half-open (the host already up, its confirmation still in flight) takes
/// the board's half-open slot, and the host's frames then fail the board's
/// CRC for ever. The host must notice the board answering someone else's
/// msg1 and start over.
#[test]
fn a_msg1_replayed_into_a_half_open_board_does_not_wedge_the_session() {
    let mut p = Pair::<NoArq>::ws(1);
    let old_msg1 = p.host.poll_transmit(0).unwrap().to_vec();
    p.feed_board(&old_msg1);
    p.run_until_up();
    p.host.restart(p.now);
    // The new handshake, up to the host's confirmation, which is held back.
    let msg1 = p.host.poll_transmit(p.now).unwrap().to_vec();
    p.feed_board(&msg1);
    p.answer_lookups();
    let msg2 = p.board.poll_transmit(p.now).unwrap().to_vec();
    p.feed_host(&msg2);
    assert_eq!(p.host.state(), LinkState::Established);
    let _held = p.host.poll_transmit(p.now).unwrap().to_vec();
    // The replay lands; the board is half-open for the dead session now.
    p.feed_board(&old_msg1);
    p.answer_lookups();
    p.drain();
    assert!(p.board.is_half_open_for_test());
    p.run_until_up();
    p.both_ways_carry_messages();
}

#[test]
fn a_new_host_session_resets_the_board_only_after_its_msg1_verifies() {
    let mut p = Pair::<SelectiveRepeat>::usb(1);
    p.run_until_up();
    let before = p.board.secure_keys_for_test().unwrap();
    p.host.restart(p.now);
    p.drain();
    assert!(
        p.host.secure_keys_for_test().is_none(),
        "a reset wipes the keys"
    );
    p.run_until_up();
    let after = p.board.secure_keys_for_test().unwrap();
    assert_ne!(before, after, "a new session has new keys");
    assert!(
        p.board_resets
            .iter()
            .any(|r| *r == ResetReason::PeerRestarted)
    );
    p.both_ways_carry_messages();
}

#[test]
fn a_rebooted_board_brings_a_fresh_handshake() {
    let mut p = Pair::<SelectiveRepeat>::usb(1);
    p.run_until_up();
    let before = p.host.secure_keys_for_test().unwrap();
    p.board = Link::new_secure(
        LinkConfig::usb(),
        0x3333_3333,
        SecureRole::Responder,
        sim_entropy::fill,
    );
    p.run_until_up();
    assert_ne!(p.host.secure_keys_for_test().unwrap(), before);
    p.both_ways_carry_messages();
}

// ---- Mixed links -------------------------------------------------------------

#[test]
fn a_plain_host_and_a_secure_board_never_come_up_and_say_so() {
    let mut p = Pair::<SelectiveRepeat>::usb(1);
    p.host = Link::new(LinkConfig::usb(), HOST_NONCE);
    p.run_for(1_000_000);
    assert_ne!(p.host.state(), LinkState::Established);
    assert_ne!(p.board.state(), LinkState::Established);
    assert!(p.host.counters().secure_required > 0);
    assert!(p.board.counters().secure_syn_ignored > 0);
}

#[test]
fn a_secure_host_and_a_plain_board_never_come_up_and_say_so_once() {
    let mut p = Pair::<SelectiveRepeat>::usb(1);
    p.board = Link::new(LinkConfig::usb(), BOARD_NONCE);
    p.run_for(1_000_000);
    assert_ne!(p.host.state(), LinkState::Established);
    assert_ne!(p.board.state(), LinkState::Established);
    assert_eq!(p.host_events, vec![SecureEvent::PeerNotSecure]);
}

// ---- Sealed frames -----------------------------------------------------------

#[test]
fn sealed_frames_hide_the_payload() {
    let mut p = Pair::<NoArq>::ws(1);
    p.run_until_up();
    let secret = b"the-plaintext-is-not-on-the-wire";
    p.host.send(CH_PROTO, secret).unwrap();
    let f = p.host.poll_transmit(p.now).unwrap().to_vec();
    assert!(!f.windows(secret.len()).any(|w| w == secret));
    p.feed_board(&f);
    assert_eq!(p.board_messages(), vec![secret.to_vec()]);
}

#[test]
fn a_forged_frame_is_dropped_on_arq_and_resets_no_arq() {
    for reliable in [true, false] {
        let (forged_ok, resets) = if reliable {
            forge::<SelectiveRepeat>(LinkConfig::udp())
        } else {
            forge::<NoArq>(LinkConfig::ws())
        };
        assert!(forged_ok, "the forgery passed the CRC");
        assert_eq!(resets, u32::from(!reliable), "reliable={reliable}");
    }
}

/// Flip a ciphertext bit and fix the CRC: the tag must catch it.
fn forge<A: Arq>(cfg: LinkConfig) -> (bool, u32) {
    let mut p = Pair::<A>::with(cfg, 1);
    p.run_until_up();
    p.host.send(CH_PROTO, b"forge me").unwrap();
    let mut f = p.host.poll_transmit(p.now).unwrap().to_vec();
    let crc = p.host.config().crc;
    let key = p.host.peer_nonce().unwrap() ^ p.board.peer_nonce().unwrap();
    let n = f.len() - crc.len();
    f[10] ^= 0x01;
    let sum = crc.compute(key, &f[..n]).to_le_bytes();
    f[n..].copy_from_slice(&sum[..crc.len()]);
    let before = p.board.counters().clone();
    p.feed_board(&f);
    let after = p.board.counters().clone();
    let ok =
        after.seal_failures == before.seal_failures + 1 && after.bad_frames == before.bad_frames;
    assert!(p.board_messages().is_empty(), "nothing forged is delivered");
    (ok, after.resets - before.resets)
}

#[test]
fn a_replayed_sealed_frame_is_dropped() {
    for reliable in [true, false] {
        let (replays, delivered) = if reliable {
            replay::<SelectiveRepeat>(LinkConfig::udp())
        } else {
            replay::<NoArq>(LinkConfig::ws())
        };
        assert_eq!((replays, delivered), (1, 1), "reliable={reliable}");
    }
}

fn replay<A: Arq>(cfg: LinkConfig) -> (u32, usize) {
    let mut p = Pair::<A>::with(cfg, 1);
    p.run_until_up();
    p.host.send(CH_PROTO, b"once").unwrap();
    let f = p.host.poll_transmit(p.now).unwrap().to_vec();
    p.feed_board(&f);
    p.feed_board(&f);
    (p.board.counters().replays, p.board_messages().len())
}

#[test]
fn a_no_arq_link_resets_on_a_counter_gap() {
    let mut p = Pair::<NoArq>::ws(1);
    p.run_until_up();
    p.host.send(CH_PROTO, b"lost").unwrap();
    p.host.send(CH_PROTO, b"after").unwrap();
    let _lost = p.host.poll_transmit(p.now).unwrap().to_vec();
    let next = p.host.poll_transmit(p.now).unwrap().to_vec();
    p.feed_board(&next);
    assert_eq!(p.board.counters().counter_gaps, 1);
    assert!(
        p.board_messages().is_empty(),
        "nothing after a gap is delivered"
    );
    assert!(p.board_resets.contains(&ResetReason::ProtocolError));
    p.run_until_up();
}

#[test]
fn the_counter_space_running_out_restarts_the_link() {
    let mut p = Pair::<SelectiveRepeat>::usb(1);
    p.run_until_up();
    p.host.set_send_counter_for_test(u32::MAX - 2);
    for i in 0..8u8 {
        p.host.send(CH_PROTO, &[i; 4]).unwrap();
    }
    p.run_for(200_000);
    assert!(p.host_resets.contains(&ResetReason::Requested));
    p.run_until_up();
    // What went before the restart either arrived or was reported by it.
    p.drain();
    p.board_inbox.clear();
    p.both_ways_carry_messages();
}

#[test]
fn a_secured_ble_link_comes_up_and_carries_a_large_message() {
    let mut p = Pair::<SelectiveRepeat>::with(LinkConfig::ble().secured(), 1);
    p.run_until_up();
    let big: Vec<u8> = (0..5_000u32).map(|i| (i * 13) as u8).collect();
    p.host.send(CH_PROTO, &big).unwrap();
    p.run_for(5_000_000);
    assert_eq!(p.board_messages(), vec![big]);
}

// ---- The harness ----------------------------------------------------------------

struct Pair<A: Arq> {
    host: Link<A>,
    board: Link<A>,
    framing: Framing,
    now: Micros,
    /// The board's key table (its edge's `installed_secrets`).
    table: Vec<(KeyId, Vec<Psk>)>,
    /// The board's edge answers a lookup this long after it was raised;
    /// `None`: never.
    answer_after: Option<Micros>,
    /// The board's edge is in login backoff: every lookup refused so.
    backoff_ms: Option<u32>,
    waiting: Vec<(KeyId, Micros)>,
    lookups: Vec<KeyId>,
    host_events: Vec<SecureEvent>,
    board_events: Vec<SecureEvent>,
    host_resets: Vec<ResetReason>,
    board_resets: Vec<ResetReason>,
    board_inbox: Vec<Vec<u8>>,
    host_inbox: Vec<Vec<u8>>,
    drop_host_frames: u32,
}

impl Pair<SelectiveRepeat> {
    fn usb(host_key: u8) -> Self {
        Self::with(LinkConfig::usb(), host_key)
    }
}

impl Pair<NoArq> {
    fn ws(host_key: u8) -> Self {
        Self::with(LinkConfig::ws(), host_key)
    }
}

impl<A: Arq> Pair<A> {
    fn with(cfg: LinkConfig, host_key: u8) -> Self {
        sim_entropy::seed(0xC0FFEE ^ u64::from(host_key));
        let (key_id, psk) = key(host_key);
        Pair {
            host: Link::new_secure(
                cfg.clone(),
                HOST_NONCE,
                SecureRole::Initiator { key_id, psk },
                sim_entropy::fill,
            ),
            board: Link::new_secure(
                cfg.clone(),
                BOARD_NONCE,
                SecureRole::Responder,
                sim_entropy::fill,
            ),
            framing: cfg.framing,
            now: 0,
            table: vec![(key(1).0, vec![key(1).1]), (key(2).0, vec![key(2).1])],
            answer_after: Some(0),
            backoff_ms: None,
            waiting: Vec::new(),
            lookups: Vec::new(),
            host_events: Vec::new(),
            board_events: Vec::new(),
            host_resets: Vec::new(),
            board_resets: Vec::new(),
            board_inbox: Vec::new(),
            host_inbox: Vec::new(),
            drop_host_frames: 0,
        }
    }

    fn feed_board(&mut self, f: &[u8]) {
        match self.framing {
            Framing::Stream => self.board.on_bytes(self.now, f),
            Framing::Datagram => self.board.on_datagram(self.now, f),
        }
        self.drain();
    }

    fn feed_host(&mut self, f: &[u8]) {
        match self.framing {
            Framing::Stream => self.host.on_bytes(self.now, f),
            Framing::Datagram => self.host.on_datagram(self.now, f),
        }
        self.drain();
    }

    /// Collect events from both links; queue the board's lookups.
    fn drain(&mut self) {
        while let Some(ev) = self.host.poll_secure_event() {
            self.host_events.push(ev);
        }
        while let Some(ev) = self.board.poll_secure_event() {
            match ev {
                SecureEvent::KeyLookup { key_id } => {
                    self.lookups.push(key_id);
                    self.waiting.push((key_id, self.now));
                }
                other => self.board_events.push(other),
            }
        }
        while let Some(ev) = self.host.recv() {
            match ev {
                LinkEvent::Message { data, .. } => self.host_inbox.push(data),
                LinkEvent::Reset { reason, .. } => self.host_resets.push(reason),
                _ => {}
            }
        }
        while let Some(ev) = self.board.recv() {
            match ev {
                LinkEvent::Message { data, .. } => self.board_inbox.push(data),
                LinkEvent::Reset { reason, .. } => self.board_resets.push(reason),
                _ => {}
            }
        }
    }

    /// The board's edge: answer every lookup that is due.
    fn answer_lookups(&mut self) {
        self.drain();
        let Some(after) = self.answer_after else {
            return;
        };
        let now = self.now;
        let due: Vec<KeyId> = self
            .waiting
            .iter()
            .filter(|(_, at)| now >= at + after)
            .map(|(k, _)| *k)
            .collect();
        self.waiting.retain(|(_, at)| now < at + after);
        for key_id in due {
            if let Some(ms) = self.backoff_ms {
                self.board.refuse(key_id, RefusalReason::Backoff, ms);
                continue;
            }
            match self.table.iter().find(|(k, _)| *k == key_id) {
                Some((_, psks)) => self.board.provide_keys(key_id, psks),
                None => self.board.refuse(key_id, RefusalReason::UnknownKey, 0),
            }
        }
    }

    /// Exchange everything due now, both ways.
    fn step(&mut self) {
        for _ in 0..64 {
            self.answer_lookups();
            let mut moved = false;
            if let Some(f) = self.host.poll_transmit(self.now) {
                let f = f.to_vec();
                moved = true;
                if self.drop_host_frames > 0 {
                    self.drop_host_frames -= 1;
                } else {
                    self.feed_board(&f);
                }
            }
            if let Some(f) = self.board.poll_transmit(self.now) {
                let f = f.to_vec();
                moved = true;
                self.feed_host(&f);
            }
            if !moved {
                break;
            }
        }
    }

    fn run_for(&mut self, us: Micros) {
        let end = self.now + us;
        while self.now < end {
            self.step();
            self.now += 1_000;
        }
    }

    fn run_until_up(&mut self) {
        for _ in 0..20_000 {
            self.step();
            if self.host.state() == LinkState::Established
                && self.board.state() == LinkState::Established
            {
                return;
            }
            self.now += 1_000;
        }
        panic!(
            "not up: host {:?} board {:?}\nhost {:?}\nboard {:?}",
            self.host.state(),
            self.board.state(),
            self.host.counters(),
            self.board.counters()
        );
    }

    fn board_messages(&mut self) -> Vec<Vec<u8>> {
        self.drain();
        std::mem::take(&mut self.board_inbox)
    }

    fn both_ways_carry_messages(&mut self) {
        let up = b"request".to_vec();
        let down: Vec<u8> = (0..3_000u32).map(|i| i as u8).collect();
        self.host.send(CH_PROTO, &up).unwrap();
        self.board.send(CH_PROTO, &down).unwrap();
        self.run_for(500_000);
        self.drain();
        assert_eq!(std::mem::take(&mut self.board_inbox), vec![up]);
        assert_eq!(std::mem::take(&mut self.host_inbox), vec![down]);
    }
}
