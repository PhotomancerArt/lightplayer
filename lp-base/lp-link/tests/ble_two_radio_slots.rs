//! The two-radio-slot RAM cost (D10, plan `lp2025/2026-09-28-1445-ble-on-lp-link`).
//!
//! `LinkConfig::ble()` itself cannot shrink `send_budget`/`keep_reassembly`
//! the way `link_config.rs`'s doc comment explains (it is shared by this
//! crate's own generic fuzzer, comms-lab soak and no-steady-state-alloc
//! guarantee, all of which push traffic past what a tight board budget would
//! take). A real board narrows those two at the firmware layer instead,
//! exactly as `UsbLinkShared::config()` narrows `usb()` for the C6's USB
//! link (`lp-fw/fw-esp32-common/src/usb_link/usb_link_shared.rs`) — replies
//! serialized once into a buffer the firmware already owns and queued with
//! [`Link::send_external`], so the send ring only ever carries a hello, a
//! heartbeat, a login exchange or a `SetEncoding` answer.
//!
//! [`board_shaped_ble_config`] previews that narrowing (P3's actual job is
//! to implement it in `fw-esp32-common`, mirroring `UsbLinkShared::config()`)
//! so this test can report the number P3 and the director need: what two of
//! these, held open at once (`RADIO_LINK_SLOTS = 2`,
//! `lp-fw/fw-esp32-common/src/radio_link/radio_link_port.rs`), actually
//! cost — real, measured, not guessed.

use lp_link::{CH_CONTROL, CH_PROTO, Link, LinkConfig, LinkState, Micros, SelectiveRepeat};

/// The region-1 floor this sizing has to respect: the largest free block
/// with BLE on and idle, no lp-link BLE traffic yet
/// (`2026-09-27-1218-fragmentation-tolerant-reads/REPORT.md`, quoted in this
/// plan's `notes.md`). Not asserted against — reported alongside the
/// measured figure so the director can judge R1 (drop `RADIO_LINK_SLOTS` to
/// 1?) with both numbers in hand.
const REGION_1_FLOOR_BYTES: usize = 19_500;

/// A preview of the board-level BLE config P3 is expected to add to
/// `fw-esp32-common`, the same shape as `UsbLinkShared::config()`: small
/// `send_budget`/`send_queue` (replies go via `send_external`, not the send
/// ring) and a small `keep_reassembly` (a rare large incoming write grows it
/// transiently and it is released right after delivery).
fn board_shaped_ble_config() -> LinkConfig {
    LinkConfig {
        // One tx window (8 x 180 B = 1,440 B) plus a small message queued
        // behind it: a hello, a heartbeat, a login exchange, or a
        // `SetEncoding` answer. Real replies never touch this ring.
        send_budget: 2_560,
        send_queue: 4,
        // A large panel/project write grows this transiently; it is
        // released once delivered, so it does not pin `max_message` bytes
        // for the radio link's whole life.
        keep_reassembly: 1024,
        ..LinkConfig::ble()
    }
}

/// One board-shaped radio slot, and the (unconstrained, host-shaped) peer it
/// talks to: Studio's own Bluetooth session, over `LinkConfig::ble()`
/// unmodified.
struct RadioSlot {
    board: Link<SelectiveRepeat>,
    peer: Link<SelectiveRepeat>,
    reply: Vec<u8>,
    now: Micros,
}

/// A reply this large is representative of a small panel/project read: well
/// under `max_message` (17 KiB), well over `send_budget`, so it can only be
/// answered by `send_external`.
const REPLY_LEN: usize = 2_048;

/// A request from the peer: a small proto-channel write, the kind of
/// traffic that does fit the send ring.
const REQUEST_LEN: usize = 96;

impl RadioSlot {
    fn new(nonce_board: u32, nonce_peer: u32) -> Self {
        let cfg = board_shaped_ble_config();
        assert_eq!(cfg.validate(), Ok(()));
        RadioSlot {
            board: Link::new(cfg, nonce_board),
            peer: Link::new(LinkConfig::ble(), nonce_peer),
            reply: (0..REPLY_LEN).map(|i| (i * 3 + 1) as u8).collect(),
            now: 0,
        }
    }

    /// Move every frame each way, no faults: this test measures RAM, not
    /// delivery under loss (that's `delivery_properties.rs`,
    /// `lab_over_sim.rs`).
    fn shuttle(&mut self) {
        let reply = &self.reply;
        let mut source = |off: usize, out: &mut [u8]| {
            out.copy_from_slice(&reply[off..off + out.len()]);
        };
        for _ in 0..16 {
            let mut moved = false;
            while let Some(f) = self.board.poll_transmit_with(self.now, &mut source) {
                self.peer.on_datagram(self.now, f);
                moved = true;
            }
            while let Some(f) = self.peer.poll_transmit(self.now) {
                self.board.on_datagram(self.now, f);
                moved = true;
            }
            if !moved {
                break;
            }
        }
    }

    fn drain(&mut self) {
        while self.board.recv().is_some() {}
        while self.peer.recv().is_some() {}
    }

    fn handshake(&mut self) {
        for _ in 0..200 {
            self.shuttle();
            self.now += 15_000;
            if self.board.state() == LinkState::Established
                && self.peer.state() == LinkState::Established
            {
                self.drain();
                return;
            }
        }
        panic!("no handshake");
    }

    /// A hello on the control channel (board -> peer), the board's answer to
    /// a `SetEncoding` (peer -> board) and one request/reply round trip
    /// (peer -> board plain `send`, board -> peer `send_external`).
    fn round_trip(&mut self) {
        self.board
            .send(CH_CONTROL, b"{\"hello\":{\"proto\":31}}")
            .expect("hello fits the send ring");
        self.peer
            .send(CH_PROTO, &self.reply[..REQUEST_LEN])
            .expect("a small request fits the peer's ring");
        self.shuttle();
        self.drain();

        self.board
            .send_external(CH_PROTO, REPLY_LEN)
            .expect("the reply fits max_message and starts external");
        while self.board.external_in_flight() {
            self.shuttle();
        }
        self.drain();
        assert_eq!(self.board.counters().resets, 0);
        assert_eq!(self.peer.counters().resets, 0);
    }

    /// A rare large incoming write (a panel/project push), sent whole
    /// (fragmented by the link, not the application) from the peer to the
    /// board over plain `send` — the peer's `LinkConfig::ble()` send_budget
    /// covers it. Returns the board's `ram_bytes()` while the message is
    /// still reassembling (the reassembly buffer grown past
    /// `keep_reassembly`, not yet released) so the transient peak this
    /// preset's `keep_reassembly` is meant to bound is a real measurement,
    /// not a guess.
    fn large_incoming_write_peak(&mut self) -> usize {
        let big: Vec<u8> = (0..LARGE_WRITE_LEN).map(|i| (i * 5 + 2) as u8).collect();
        self.peer
            .send(CH_PROTO, &big)
            .expect("the peer's generous send_budget covers one large write");
        let mut peak = self.board.ram_bytes();
        // Shuttle a few frames at a time so the peak is read mid-reassembly,
        // not only once the whole message has already arrived and released.
        for _ in 0..64 {
            self.shuttle();
            peak = peak.max(self.board.ram_bytes());
            if self.board.buffered_bytes() == 0 && self.peer.is_idle() {
                break;
            }
        }
        self.drain();
        peak
    }
}

/// A large incoming write, big enough to force the reassembly buffer past
/// `keep_reassembly` (1 KiB) but well under `max_message` (17 KiB).
const LARGE_WRITE_LEN: usize = 6 * 1024;

/// Two board-shaped radio slots, both occupied at once — `RADIO_LINK_SLOTS`
/// (2) fully in use. Prints the combined figure `--nocapture` shows; the
/// director's ruling on R1 (drop to 1 slot if this is not affordable) reads
/// off this number.
#[test]
fn two_occupied_radio_slots_cost_this_much_ram() {
    let mut a = RadioSlot::new(0x1111_1111, 0x2222_2222);
    let mut b = RadioSlot::new(0x3333_3333, 0x4444_4444);
    a.handshake();
    b.handshake();

    let at_rest_a = a.board.ram_bytes();
    let at_rest_b = b.board.ram_bytes();
    let at_rest = at_rest_a + at_rest_b;

    for _ in 0..3 {
        a.round_trip();
        b.round_trip();
    }
    let mid_traffic_a = a.board.ram_bytes();
    let mid_traffic_b = b.board.ram_bytes();
    let mid_traffic = mid_traffic_a + mid_traffic_b;

    // A large incoming write on each slot, to measure the reassembly
    // buffer's real transient peak (what `keep_reassembly` bounds), not just
    // the settled figure above.
    let peak_write_a = a.large_incoming_write_peak();
    let peak_write_b = b.large_incoming_write_peak();
    let peak_write = peak_write_a + peak_write_b;
    let after_release_a = a.board.ram_bytes();
    let after_release_b = b.board.ram_bytes();
    let after_release = after_release_a + after_release_b;

    let cfg = board_shaped_ble_config();
    let bound_each = Link::<SelectiveRepeat>::ram_bound(&cfg);
    let bound_combined = bound_each * 2;

    std::println!(
        "two board-shaped BLE radio slots (RADIO_LINK_SLOTS=2), \
         send_budget={} B, keep_reassembly={} B, max_payload={} B:",
        cfg.send_budget,
        cfg.keep_reassembly,
        cfg.max_payload
    );
    std::println!(
        "  at rest:     slot A {at_rest_a} B + slot B {at_rest_b} B = {at_rest} B combined"
    );
    std::println!(
        "  mid-traffic: slot A {mid_traffic_a} B + slot B {mid_traffic_b} B = {mid_traffic} B combined"
    );
    std::println!(
        "  peak during a {LARGE_WRITE_LEN} B incoming write: slot A {peak_write_a} B + slot B {peak_write_b} B = {peak_write} B combined"
    );
    std::println!(
        "  after release: slot A {after_release_a} B + slot B {after_release_b} B = {after_release} B combined"
    );
    std::println!(
        "  worst case (ram_bound): one slot {bound_each} B, two slots {bound_combined} B"
    );
    std::println!("  region-1 floor (BLE on, idle, before this plan): {REGION_1_FLOOR_BYTES} B");
    for (label, combined) in [
        ("at-rest", at_rest),
        ("mid-traffic", mid_traffic),
        ("large-write peak", peak_write),
    ] {
        if combined >= REGION_1_FLOOR_BYTES {
            std::println!(
                "  >>> {label} combined cost ({combined} B) already exceeds the region-1 \
                 floor ({REGION_1_FLOOR_BYTES} B) reported before this plan added anything \
                 -- see R1 in the plan's director rulings."
            );
        }
    }

    assert!(at_rest > 0 && mid_traffic > 0 && peak_write > 0);
    assert!(
        peak_write <= bound_combined,
        "{peak_write} > {bound_combined}"
    );
    assert!(
        after_release <= at_rest + 512,
        "the reassembly growth released back down: {after_release} vs at-rest {at_rest}"
    );
}

/// Sanity: the board-shaped config still validates and still carries the
/// wire's global message budget (D4 keeps `max_message` global, not
/// something BLE narrows).
#[test]
fn the_board_shaped_config_holds_together_and_keeps_the_global_message_budget() {
    let cfg = board_shaped_ble_config();
    assert_eq!(cfg.validate(), Ok(()));
    assert_eq!(cfg.max_message, lp_link::MAX_MESSAGE);
    assert!(
        cfg.send_budget >= cfg.tx_window as usize * cfg.max_payload as usize + 512,
        "the ring holds a full window plus a notice"
    );
}

/// A reply above `send_budget` can only go external, never through the
/// plain send ring — the whole point of the board-shaped config.
#[test]
fn a_reply_above_send_budget_only_fits_as_an_external_message() {
    let cfg = board_shaped_ble_config();
    let mut link = Link::<SelectiveRepeat>::new(cfg.clone(), 1);
    let big = vec![0u8; cfg.send_budget + 256];
    assert!(matches!(
        link.send(CH_PROTO, &big),
        Err(lp_link::SendError::TooBig)
    ));
    assert!(link.send_external(CH_PROTO, big.len()).is_ok());
}
