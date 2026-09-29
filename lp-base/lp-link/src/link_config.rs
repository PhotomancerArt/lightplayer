//! A link's tuning, with a preset per transport. Presets are starting points
//! from the simulator (`link-bench`); see the M2 report for the evidence.

use crate::Micros;
use crate::crc::CrcKind;

/// Channel 0: link-adjacent control messages (hello, version). Reliable.
pub const CH_CONTROL: u8 = 0;
/// Channel 1: the wire protocol's messages. Reliable.
pub const CH_PROTO: u8 = 1;
/// Channel 2: structured log lines. Best effort: dropped, not retried.
pub const CH_LOG: u8 = 2;

/// The presets' `max_message`: the wire's 16 KiB frame budget plus 1 KiB.
pub const MAX_MESSAGE: usize = 17 * 1024;

/// How frames meet the transport.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Framing {
    /// A byte stream (USB serial, UART, BLE used as a byte pipe): frames are
    /// COBS-encoded between `0x00` delimiters, and bytes outside frames are
    /// text. Feed [`Link::on_bytes`](crate::Link::on_bytes).
    Stream,
    /// A message transport (a BLE notification or write, a UDP datagram, a
    /// WebSocket message): one frame per datagram, no COBS, no delimiters.
    /// Feed [`Link::on_datagram`](crate::Link::on_datagram).
    Datagram,
}

#[derive(Clone, Debug)]
pub struct LinkConfig {
    pub framing: Framing,
    pub crc: CrcKind,
    /// Largest payload in one frame; a longer message is fragmented. Both ends
    /// use the smaller of their two values.
    pub max_payload: u16,
    /// Frames in flight before an acknowledgement.
    pub tx_window: u8,
    /// Frames this end takes past its cumulative ack (selective repeat's
    /// reorder buffer; the flow-control ceiling for the others).
    pub rx_window: u8,
    /// Message bytes queued for the application before the advertised window
    /// closes (each queued message is also charged a small fixed cost, see
    /// `Link::ram_bound`). Not allocated up front: delivered messages are the
    /// application's `Vec`s.
    pub rx_budget: usize,
    /// Bytes `send()` queues (pending + unacknowledged) before `Full`. The
    /// pending part is a ring of this size, allocated once in `Link::new`.
    /// External messages ([`Link::send_external`](crate::Link::send_external))
    /// take no room in it; their fragments in the transmit window still count.
    pub send_budget: usize,
    /// Reliable messages `send()` queues (not yet cut into frames) before
    /// `Full`.
    pub send_queue: usize,
    /// Longest reliable message. `send` refuses a longer one, or one longer
    /// than `send_budget` (`TooBig`); a
    /// longer one arriving (a peer with a bigger limit) is dropped and counted
    /// (`LinkCounters::oversize_messages`), and the session carries on.
    pub max_message: usize,
    /// After a reassembled message is delivered, a channel's reassembly
    /// buffer whose capacity is above this is released, so one large message
    /// does not pin `max_message` bytes for the link's life. The presets keep
    /// it at `max_message` (a buffer grown once is kept); a RAM-tight board
    /// sets it small.
    pub keep_reassembly: usize,
    /// Best-effort messages queued before `Full`: this many `max_payload`
    /// slots, allocated once.
    pub datagram_queue: usize,
    /// Fair share for best-effort messages (logs): after this many reliable
    /// data frames in a row, a queued datagram goes next, so a busy proto
    /// stream cannot starve the log. 0: datagrams go only when no reliable
    /// frame can.
    pub datagram_every: u8,
    /// Bit `n` set: channel `n` is reliable.
    pub reliable_channels: u8,
    /// Hold an ACK this long hoping to ride on data.
    pub ack_delay: Micros,
    /// ...but ACK at once after this many unacknowledged frames.
    pub ack_every: u8,
    /// Resend a hole early once this many frames sent after it are
    /// acknowledged (selective repeat). 1 on a transport that never reorders;
    /// more where it does (UDP), so reordering is not mistaken for loss.
    pub reorder_threshold: u8,
    pub initial_rto: Micros,
    pub min_rto: Micros,
    pub max_rto: Micros,
    /// Sends of one frame before the link gives up and restarts.
    pub max_retries: u8,
    pub syn_interval: Micros,
    /// Send an empty ACK after this much transmit silence.
    pub keepalive: Micros,
    /// Nothing heard for this long: the peer is stalled (a cable out, a hung
    /// page). The session is kept, but logs stay in the ring instead of going
    /// out to nobody. A few keepalives' worth.
    pub stall_after: Micros,
    /// Hand up unterminated console text after this much quiet.
    pub idle_flush: Micros,
    /// Abandon a partial frame after this much quiet (stream framing). Much
    /// longer than `idle_flush` on purpose: a busy end writes one frame in
    /// pieces (the C6 yields to an 80 ms render tick between 64-byte USB
    /// packets; a page's read pump can sit behind a long task), and a frame
    /// abandoned mid-way is resent only to be split and abandoned again — the
    /// 2026-09-27 rehearsal saw 4 s stalls every palette cross-fade at 50 ms.
    /// A frame that really lost its tail is caught anyway, by the next `0x00`
    /// and the CRC; this only decides when a quiet partial stops waiting.
    pub frame_abandon: Micros,
    /// Stream framing only: keep `0xFF` off the wire too (COBS-FF, see
    /// [`cobs`](crate::cobs)). On everywhere; off only to measure what
    /// plain COBS costs through Chromium's Web Serial on macOS (M3's A/B).
    pub escape_ff: bool,
}

impl LinkConfig {
    /// USB CDC / USB-Serial-JTAG: ~1 ms latency, 64-byte packets.
    pub fn usb() -> Self {
        LinkConfig {
            framing: Framing::Stream,
            crc: CrcKind::Crc32c,
            max_payload: 256,
            tx_window: 8,
            rx_window: 8,
            // Room for one largest message plus what queues behind it. The
            // send ring is allocated at `send_budget` for the link's life, so
            // it is kept near one message: `send()` says `Full` rather than
            // hold more.
            rx_budget: 24 * 1024,
            send_budget: 24 * 1024,
            send_queue: 64,
            // The wire's one message budget, 16 KiB
            // (`lpc_wire::budget::PROJECT_READ_FRAME_MAX_BYTES` in
            // lp-core/lpc-wire/src/budget.rs; its serial margin is 256 B),
            // plus 1 KiB of slack. lp-base cannot depend on lp-core, so the
            // edge that wires the link to the wire asserts the two agree.
            max_message: MAX_MESSAGE,
            keep_reassembly: MAX_MESSAGE,
            datagram_queue: 32,
            datagram_every: 4,
            reliable_channels: (1 << CH_CONTROL) | (1 << CH_PROTO),
            ack_delay: 1_000,
            ack_every: 2,
            reorder_threshold: 1,
            initial_rto: 50_000,
            // 40 ms, not M2's 10: on the emulated C6 under echo + stream
            // load a frame's round trip reaches 10–20 ms (64-byte packets
            // queued behind a 2 KB window each way), and at 10 ms 2% of
            // frames were resent with nothing lost. At 40 ms: 0.06%, the
            // same goodput. Loss is found early by SACK and the tail probe;
            // the timer is the backstop. (M3, lp-emu:esp32c6:t1.)
            min_rto: 40_000,
            max_rto: 1_000_000,
            max_retries: 20,
            syn_interval: 100_000,
            keepalive: 250_000,
            stall_after: 1_000_000,
            idle_flush: 50_000,
            frame_abandon: 3_000_000,
            escape_ff: true,
        }
    }

    /// BLE NUS: a 15–30 ms connection interval, one frame per notification or
    /// write.
    ///
    /// This preset is shared by every consumer of `ble()` — the C6 board,
    /// but also this crate's own generic reliability-property fuzzer
    /// (`delivery_properties.rs`, `link_scenarios.rs`'s `random()`), the
    /// comms-lab soak (`lab_over_sim.rs`'s BLE case), and the no-steady-
    /// state-allocation guarantee (`no_steady_state_alloc.rs`'s BLE case),
    /// which run every `Transport` variant through the plain `Link::send()`
    /// path with messages up to 16 KiB (well past what a board sends
    /// through it — real replies go via `send_external`) and, for the
    /// alloc test, repeatedly at steady state. That is why `send_budget`,
    /// `max_message` and `keep_reassembly` stay at `usb()`'s values here
    /// (`..Self::usb()`): a `keep_reassembly` below `max_message` reallocates
    /// the reassembly buffer on every large message past warm-up instead of
    /// keeping it — exactly what `no_steady_state_alloc` forbids — and a
    /// `send_budget` below ~16 KiB makes the comms-lab's default soak size
    /// `TooBig`. Both were verified by running them, not guessed. USB's own
    /// preset has the identical shape: `usb()` stays generous, and the
    /// board-specific narrowing (`send_budget` 2,560 B, `keep_reassembly`
    /// 1 KiB, replies via `send_external`) lives in firmware only
    /// (`UsbLinkShared::config()`,
    /// `lp-fw/fw-esp32-common/src/usb_link/usb_link_shared.rs`). A BLE
    /// firmware config doing the same is P3's job, not this preset's — see
    /// this phase's Implementation Result for the two-radio-slot RAM figure
    /// measured against that board-shaped config, and for the contradiction
    /// this raised against the phase brief's original plan to narrow
    /// `send_budget`/`keep_reassembly` directly here.
    pub fn ble() -> Self {
        LinkConfig {
            framing: Framing::Datagram,
            // 180 B, not the ATT MTU-derived 236: `browser_ble.js`'s write
            // chunker has used 180 B since M2 (safely under every measured
            // usable MTU — iOS 185→182, macOS/board 247→244) and at 180 B
            // every lp-link frame (4 header + payload + 4 CRC = 188 B raw)
            // fits inside one ATT write or notification, always. No frame is
            // ever split across an ATT long write (Prepare…Execute), which
            // retires `prepared_write.rs` entirely (D3/D7,
            // `lp2025/2026-09-28-1445-ble-on-lp-link`). Director ruling R4
            // (2026-09-28): keep 180 B.
            max_payload: 180,
            tx_window: 8,
            rx_window: 8,
            // `rx_budget` is NOT a preallocated buffer (see the field doc):
            // it only bounds the inbox's worst case and `validate()`
            // (`max_message + EVENT_COST <= rx_budget`). Shrunk from
            // `usb()`'s 24 KiB to the tightest value that still holds one
            // largest message plus its queueing charge — unlike
            // `send_budget`/`keep_reassembly` above, nothing in this crate's
            // own tests sends enough concurrent unread traffic to notice.
            rx_budget: MAX_MESSAGE + crate::inbox::EVENT_COST,
            // BLE log traffic is lower-priority and lower-volume than USB's
            // (32 slots): eight `max_payload`-sized slots (one per tx-window
            // frame) is enough buffering for the board's structured logs
            // without holding a whole extra `max_payload × 32` allocation
            // per radio link.
            datagram_queue: 8,
            ack_delay: 15_000,
            ack_every: 4,
            initial_rto: 500_000,
            // RTT is quantized by 30 ms connection events and a queued
            // notification can wait two of them; below ~2x that, timers fire
            // on frames that are merely queued.
            min_rto: 250_000,
            max_rto: 3_000_000,
            syn_interval: 500_000,
            keepalive: 1_000_000,
            stall_after: 3_500_000,
            idle_flush: 500_000,
            ..Self::usb()
        }
    }

    /// UDP on a LAN: one frame per datagram, loss, reordering, duplication.
    pub fn udp() -> Self {
        LinkConfig {
            framing: Framing::Datagram,
            max_payload: 1024,
            tx_window: 16,
            rx_window: 16,
            rx_budget: 32 * 1024,
            ack_delay: 5_000,
            ack_every: 4,
            reorder_threshold: 3,
            initial_rto: 300_000,
            min_rto: 20_000,
            max_rto: 3_000_000,
            syn_interval: 300_000,
            keepalive: 1_000_000,
            stall_after: 3_500_000,
            idle_flush: 300_000,
            ..Self::usb()
        }
    }

    /// WebSocket (or any reliable, ordered transport). Pair it with the
    /// no-ARQ variant: the transport already retransmits.
    pub fn ws() -> Self {
        LinkConfig {
            max_payload: 1024,
            reorder_threshold: 1,
            ..Self::udp()
        }
    }

    pub fn is_reliable(&self, channel: u8) -> bool {
        channel < 8 && self.reliable_channels & (1 << channel) != 0
    }

    /// The budgets hold together: one largest message, with its queueing
    /// charge, fits the receive budget (a message that can never fit would
    /// stall the link until it resets); a frame carries a payload; the
    /// datagram queue has a slot. The send budget may be smaller than
    /// `max_message`: `send` refuses a message longer than it (`TooBig`), and a
    /// larger one goes by [`Link::send_external`](crate::Link::send_external).
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.max_payload == 0 {
            return Err("max_payload is 0");
        }
        if self.max_message + crate::inbox::EVENT_COST > self.rx_budget {
            return Err("max_message (plus its queueing charge) does not fit rx_budget");
        }
        if self.send_queue == 0 || self.datagram_queue == 0 {
            return Err("send_queue and datagram_queue need at least one slot");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_holds_together() {
        for (name, cfg) in [
            ("usb", LinkConfig::usb()),
            ("ble", LinkConfig::ble()),
            ("udp", LinkConfig::udp()),
            ("ws", LinkConfig::ws()),
        ] {
            assert_eq!(cfg.validate(), Ok(()), "{name}");
            assert_eq!(cfg.max_message, MAX_MESSAGE, "{name}");
        }
    }
}
