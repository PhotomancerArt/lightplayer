//! The `espnow-broadcast` payload: two boards on the air, each saying what it
//! sent and what it heard.
//!
//! The harness this migrates (`fw-esp32c6/src/tests/test_espnow.rs`) has
//! broadcast a simulated button press every second since long before there was
//! a validation system, and printed four `{:?}`-formatted lines that no parser
//! could hold on to. It stays where it is — deleting a file under `lp-fw/`
//! outside `fw-checks` is E-product (RD13/DD31) — and what moves here are its
//! *facts*: the broadcast cadence, the diagnostic channel, the device id, the
//! event counter, and the four fields a receiver can say about a frame.
//!
//! Everything in this module is arithmetic over bytes: the schedule, the
//! payload-length ladder, the record shapes, the sentinel, and the
//! phase-independent reduction of a peer's event counter that makes two
//! captures comparable. `no_std`, `alloc`-free, host-tested. What stays in
//! `fw-esp32c6` is board init, the ESP-NOW driver, the hardware registry and
//! the executor — the parts that need a chip.
//!
//! # The two lines per event, and why there are two
//!
//! Each event prints a human line **and** a structured record:
//!
//! ```text
//! [espnow-broadcast] tx device=0x8cb48762 event=1 kind=1 payload_len=0
//! [fw-check-json] {"kind":"espnow-tx","n":0,"device":2360641378,"event":1,"msg_kind":1,"payload_len":0}
//! [espnow-broadcast] rx device=0x7ca88562 event=17 kind=1 payload_len=8
//! [fw-check-json] {"kind":"espnow-rx","n":0,"peer":2091418978,"gap":0,"msg_kind":1,"len_ok":true}
//! ```
//!
//! The human line is `test_espnow`'s own spelling with the identities spelled
//! as numbers rather than as `{:?}` of a newtype, and it is the line the
//! roadmap's acceptance criterion is written in: *each machine prints the
//! other's `event=N` lines*. The record is what a replay compares.
//!
//! # Why the record does not carry the peer's raw event number
//!
//! It cannot compare, and pretending otherwise would make every capture red
//! for a reason that says nothing about a model.
//!
//! A receiver's own `tx` counter starts at 1 at its own power-on, so `event`
//! on the **tx** record compares exactly. The peer's counter does not: on the
//! desk, two boards are captured one after the other through one port, so the
//! board that is *not* being recorded has been powered — and counting — since
//! the previous flash. Board A's transcript sees board B's event 45; the
//! emulated pair's machine A sees machine B's event 2. Same air, same frames,
//! different origin.
//!
//! So the record carries what does not depend on the origin:
//!
//! * [`RxRecord::gap`] — this event's number minus the previous one's from the
//!   same peer, and `0` for the first. A stream with no losses is all `1`s on
//!   both sides, whatever it started from. It is the honest content of a
//!   counter whose origin cannot be aligned: *nothing was dropped between
//!   these two frames*.
//! * [`RxRecord::len_ok`] — whether the payload length is the one the peer's
//!   own event number prescribes ([`payload_len_for`]). The ladder is a
//!   function of the event number, so a receiver can check the byte count
//!   end to end without knowing where the sender's counter started. Raw
//!   lengths would be phase-shifted for exactly the reason raw events are.
//!
//! The raw `event` and the raw `payload_len` are still in the transcript, on
//! the human line, verbatim and never edited. The payload's mask set names
//! them as the two identity fields it hides for the human view, with this
//! reason.
//!
//! # The payload-length ladder, and the unknown it exists to settle
//!
//! M4 P0 found the ESP-NOW TX descriptor's word bits `[23:12]` undetermined
//! and could not settle them, because `test_espnow` sends **one** size and the
//! machine armed exactly one frame per boot (U2, `m4/discovery-air.md` §5).
//! Both blockers are gone — a TX completes (M4 U1) and this payload sends six
//! — so the ladder here is deliberately four different sizes:
//!
//! ```text
//! event 1 2 3 4 5 6 …
//! bytes 0 8 24 64 0 8 …
//! ```
//!
//! Four sizes in the first four events, through the capability API's own
//! `send_channel(channel, kind, payload)` and with no change to the ESP-NOW
//! driver, which is read and never written (E-product).
//!
//! # Bounded on both sides, and still broadcasting afterwards
//!
//! The sentinel arrives after [`TX_EVENTS`] sends **and** [`RX_EVENTS`]
//! received frames, and the payload keeps broadcasting for ever after it. Two
//! reasons, and they pull the same way:
//!
//! 1. A replay compares records of one kind index by index and refuses two
//!    transcripts with different counts of them. How many frames a peer got
//!    through a window is a *timing* fact, so an unbounded receiver would make
//!    every capture a different shape. Six and six is a shape.
//! 2. The desk captures two boards through one port, one after the other
//!    (`d1-desk-batch.md` step 3). The board that is not being recorded has to
//!    still be on the air, so the sentinel ends the **host's capture**, never
//!    the firmware.

use core::fmt;

/// The line that says the payload finished.
pub const DONE_MARKER: &str = "[espnow-broadcast] === DONE ===";

/// The logical channel the diagnostic traffic rides, unchanged from
/// `test_espnow`.
pub const DIAGNOSTIC_CHANNEL: u32 = 1;

/// The tick period of the payload's own loop, milliseconds.
pub const TICK_MS: u32 = 50;

/// Ticks between broadcasts: one frame every 100 ms.
///
/// `test_espnow`'s cadence is 1 Hz, which is a smoke test's cadence — it exists
/// so that a human watching a serial monitor can read it. This payload's
/// consumer is a replay, and every emulated second of it is bought with host
/// seconds twice over (a lockstep pair runs two machines), so the cadence is
/// the fastest one that is still unambiguously a sequence of separate frames
/// on a real air: ten a second, against an air time of 672 µs.
pub const TICKS_PER_SEND: u32 = 2;

/// Broadcasts before the sentinel is eligible.
pub const TX_EVENTS: u32 = 6;

/// Received frames recorded before the sentinel is eligible. Frames after the
/// sixth are still received and still drained — they are simply not recorded,
/// because a record count that depends on the window is not a shape.
pub const RX_EVENTS: u32 = 6;

/// How long the payload waits for its two counts before printing the sentinel
/// anyway, in ticks after the radio is ready.
///
/// A capture that ends early is a short transcript and a red replay, which is
/// the right outcome: it says *this run did not see its peer*, and it says it
/// in the record counts rather than by hanging until the runner's timeout and
/// producing nothing at all.
pub const DEADLINE_TICKS: u32 = 200;

/// The payload lengths the ladder walks, by event number.
pub const PAYLOAD_LENS: [usize; 4] = [0, 8, 24, 64];

/// The payload length event `event` carries. `event` is 1-based, as the
/// driver's own counter is.
pub const fn payload_len_for(event: u32) -> usize {
    if event == 0 {
        return 0;
    }
    PAYLOAD_LENS[((event - 1) % PAYLOAD_LENS.len() as u32) as usize]
}

/// The bytes event `event` carries, written into `out`.
///
/// The content is a function of the event number too, so a receiver that got
/// the right *number* of bytes from the wrong frame would still be visible in
/// a hex dump. Nothing compares it today; it costs nothing and it is the kind
/// of thing a later question wants to have been there.
pub fn fill_payload(event: u32, out: &mut [u8]) -> usize {
    let len = payload_len_for(event).min(out.len());
    for (i, slot) in out[..len].iter_mut().enumerate() {
        *slot = (event as u8).wrapping_mul(17).wrapping_add(i as u8);
    }
    len
}

/// One broadcast this device made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxRecord {
    /// 0-based index in this transcript's tx sequence.
    pub n: u32,
    /// This device's own id: the low four bytes of its station MAC, which is
    /// its eFuse MAC, which is what tells two boards of one model apart.
    pub device: u32,
    /// The driver's own 1-based event counter, from this device's power-on.
    pub event: u32,
    /// `RadioMessageKind::as_u8`.
    pub msg_kind: u8,
    pub payload_len: usize,
}

impl fmt::Display for TxRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            r#"{{"kind":"espnow-tx","n":{},"device":{},"event":{},"msg_kind":{},"payload_len":{}}}"#,
            self.n, self.device, self.event, self.msg_kind, self.payload_len
        )
    }
}

/// One frame this device heard from a peer.
///
/// See the module docs for why the peer's raw event number and raw payload
/// length are not here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxRecord {
    /// 0-based index in this transcript's rx sequence.
    pub n: u32,
    /// The sending device's id, as the frame carries it.
    pub peer: u32,
    /// This event's number minus the previous one from the same peer; `0` for
    /// the first frame heard from it. `1` means nothing was dropped.
    pub gap: u32,
    pub msg_kind: u8,
    /// Did the frame carry the number of bytes the peer's event number says it
    /// should have?
    pub len_ok: bool,
}

impl fmt::Display for RxRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            r#"{{"kind":"espnow-rx","n":{},"peer":{},"gap":{},"msg_kind":{},"len_ok":{}}}"#,
            self.n, self.peer, self.gap, self.msg_kind, self.len_ok
        )
    }
}

/// The last event number seen from each peer, so that [`RxRecord::gap`] can be
/// computed without an allocator.
///
/// Four slots: the bench has two boards, and a payload that silently ignored a
/// third peer would be lying by omission. A fifth peer is counted in
/// [`Peers::overflowed`] and reported in the summary line.
#[derive(Clone, Copy, Debug, Default)]
pub struct Peers {
    slots: [Option<(u32, u32)>; 4],
    overflowed: bool,
}

impl Peers {
    pub const fn new() -> Self {
        Self {
            slots: [None; 4],
            overflowed: false,
        }
    }

    /// Record `event` from `peer` and return the gap from the previous one,
    /// or `0` if this is the first frame from it.
    pub fn observe(&mut self, peer: u32, event: u32) -> u32 {
        for slot in &mut self.slots {
            match slot {
                Some((id, last)) if *id == peer => {
                    let gap = event.wrapping_sub(*last);
                    *last = event;
                    return gap;
                }
                _ => {}
            }
        }
        for slot in &mut self.slots {
            if slot.is_none() {
                *slot = Some((peer, event));
                return 0;
            }
        }
        self.overflowed = true;
        0
    }

    pub fn seen(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    pub const fn overflowed(&self) -> bool {
        self.overflowed
    }
}

/// The rx record for a frame, given the peer table's answer.
pub fn rx_record(n: u32, peer: u32, event: u32, msg_kind: u8, payload_len: usize, gap: u32) -> RxRecord {
    RxRecord {
        n,
        peer,
        gap,
        msg_kind,
        len_ok: payload_len == payload_len_for(event),
    }
}

/// The readiness line, printed once the radio is up.
pub fn write_ready<W: fmt::Write>(w: &mut W, device: u32, espnow_channel: u8) -> fmt::Result {
    writeln!(
        w,
        "[espnow-broadcast] radio ready device=0x{device:08x} espnow_channel={espnow_channel} \
         logical_channel={DIAGNOSTIC_CHANNEL} tick_ms={TICK_MS} ticks_per_send={TICKS_PER_SEND} \
         tx_events={TX_EVENTS} rx_events={RX_EVENTS}"
    )
}

/// The human line for a broadcast. `test_espnow`'s spelling, with the
/// identities as numbers.
pub fn write_tx_line<W: fmt::Write>(w: &mut W, record: &TxRecord) -> fmt::Result {
    writeln!(
        w,
        "[espnow-broadcast] tx device=0x{:08x} event={} kind={} payload_len={}",
        record.device, record.event, record.msg_kind, record.payload_len
    )
}

/// The human line for a received frame, carrying the two identity fields the
/// record deliberately does not: the peer's raw event number and the raw byte
/// count.
pub fn write_rx_line<W: fmt::Write>(
    w: &mut W,
    peer: u32,
    event: u32,
    msg_kind: u8,
    payload_len: usize,
) -> fmt::Result {
    writeln!(
        w,
        "[espnow-broadcast] rx device=0x{peer:08x} event={event} kind={msg_kind} \
         payload_len={payload_len}"
    )
}

/// The closing summary, before the sentinel.
pub fn write_summary<W: fmt::Write>(
    w: &mut W,
    tx: u32,
    rx: u32,
    peers: &Peers,
    dropped: u32,
) -> fmt::Result {
    writeln!(
        w,
        "[espnow-broadcast] tx={tx} rx={rx} peers={} peer_overflow={} dropped={dropped}",
        peers.seen(),
        peers.overflowed()
    )
}

/// Write the done marker.
pub fn write_done<W: fmt::Write>(w: &mut W) -> fmt::Result {
    writeln!(w, "{DONE_MARKER}")
}

/// Emit one tx record through `log`.
pub fn emit_tx_record(record: &TxRecord) {
    crate::emit_record_json(format_args!("{record}"));
}

/// Emit one rx record through `log`.
pub fn emit_rx_record(record: &RxRecord) {
    crate::emit_record_json(format_args!("{record}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate std;
    use std::string::String;

    #[test]
    fn the_ladder_walks_four_sizes_in_the_first_four_events() {
        let lens: std::vec::Vec<usize> = (1..=6).map(payload_len_for).collect();
        assert_eq!(lens, std::vec![0, 8, 24, 64, 0, 8]);
        // U2's experiment needs two *different* lengths from one boot, and it
        // has them by event 2.
        assert_ne!(payload_len_for(1), payload_len_for(2));
    }

    #[test]
    fn the_payload_bytes_are_a_function_of_the_event() {
        let mut a = [0u8; 64];
        let mut b = [0u8; 64];
        let la = fill_payload(2, &mut a);
        let lb = fill_payload(6, &mut b);
        assert_eq!(la, 8);
        assert_eq!(lb, 8);
        assert_ne!(a[..la], b[..lb], "two events of one length sent one blob");
    }

    #[test]
    fn a_contiguous_stream_is_all_ones_whatever_it_started_from() {
        let mut peers = Peers::new();
        // The desk's shape: the peer has been counting since its own flash.
        assert_eq!(peers.observe(0x7ca8_8562, 45), 0);
        for event in 46..=50 {
            assert_eq!(peers.observe(0x7ca8_8562, event), 1);
        }
        // The emulated pair's shape, same air, different origin — and the same
        // gaps, which is the whole reason the record carries this and not the
        // raw number.
        let mut emulated = Peers::new();
        assert_eq!(emulated.observe(0x7ca8_8562, 2), 0);
        for event in 3..=7 {
            assert_eq!(emulated.observe(0x7ca8_8562, event), 1);
        }
    }

    #[test]
    fn a_dropped_frame_shows_up_as_a_gap_of_two() {
        let mut peers = Peers::new();
        peers.observe(7, 10);
        assert_eq!(peers.observe(7, 11), 1);
        assert_eq!(peers.observe(7, 13), 2);
    }

    #[test]
    fn two_peers_are_counted_apart() {
        let mut peers = Peers::new();
        peers.observe(1, 100);
        peers.observe(2, 5);
        assert_eq!(peers.observe(1, 101), 1);
        assert_eq!(peers.observe(2, 6), 1);
        assert_eq!(peers.seen(), 2);
        assert!(!peers.overflowed());
    }

    #[test]
    fn a_fifth_peer_is_reported_rather_than_ignored() {
        let mut peers = Peers::new();
        for id in 1..=4 {
            peers.observe(id, 1);
        }
        assert!(!peers.overflowed());
        peers.observe(5, 1);
        assert!(peers.overflowed());
    }

    #[test]
    fn len_ok_checks_the_byte_count_against_the_peers_own_event_number() {
        // Phase-independent: the peer's event says 8 bytes and 8 arrived.
        assert!(rx_record(0, 9, 2, 1, 8, 0).len_ok);
        assert!(rx_record(0, 9, 46, 1, 8, 1).len_ok);
        // A short frame is visible however far along the peer's counter is.
        assert!(!rx_record(0, 9, 2, 1, 7, 0).len_ok);
    }

    #[test]
    fn the_records_render_the_lines_the_host_parses() {
        let tx = TxRecord {
            n: 0,
            device: 0x8cb4_8762,
            event: 1,
            msg_kind: 1,
            payload_len: 0,
        };
        let mut out = String::new();
        crate::checks::espnow_broadcast::write_tx_line(&mut out, &tx).unwrap();
        assert_eq!(
            out,
            "[espnow-broadcast] tx device=0x8cb48762 event=1 kind=1 payload_len=0\n"
        );
        assert_eq!(
            std::format!("{tx}"),
            r#"{"kind":"espnow-tx","n":0,"device":2360641378,"event":1,"msg_kind":1,"payload_len":0}"#
        );

        let rx = rx_record(0, 0x7ca8_8562, 17, 1, 0, 0);
        assert_eq!(
            std::format!("{rx}"),
            r#"{"kind":"espnow-rx","n":0,"peer":2091418978,"gap":0,"msg_kind":1,"len_ok":true}"#
        );
        let mut line = String::new();
        write_rx_line(&mut line, 0x7ca8_8562, 17, 1, 0).unwrap();
        assert_eq!(
            line,
            "[espnow-broadcast] rx device=0x7ca88562 event=17 kind=1 payload_len=0\n"
        );
    }

    /// The two desk boards' device ids, derived the way the product driver
    /// derives them (`station_device_id`: the low four bytes of the station
    /// MAC, little-endian). They are in the tests because the transcripts and
    /// the sidecars name them, and a typo in one of them would silently
    /// compare the wrong board.
    #[test]
    fn the_bench_macs_give_the_device_ids_the_transcripts_name() {
        let id = |mac: [u8; 6]| u32::from_le_bytes([mac[2], mac[3], mac[4], mac[5]]);
        assert_eq!(id([0xa0, 0xf2, 0x62, 0x87, 0xb4, 0x8c]), 0x8cb4_8762);
        assert_eq!(id([0xa0, 0xf2, 0x62, 0x85, 0xa8, 0x7c]), 0x7ca8_8562);
    }

    #[test]
    fn the_done_marker_is_the_line_the_runner_stops_on() {
        let mut out = String::new();
        write_done(&mut out).unwrap();
        assert_eq!(out, "[espnow-broadcast] === DONE ===\n");
    }
}
