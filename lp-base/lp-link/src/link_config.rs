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
/// Channel 3: firmware update chunks (OTA split-link spike). Reliable.
pub const CH_UPDATE: u8 = 3;

/// The presets' `max_message`: the wire's 16 KiB frame budget plus 1 KiB.
pub const MAX_MESSAGE: usize = 17 * 1024;

/// Bytes a secure link's sealing adds to every frame after the handshake:
/// a 4-byte counter and a 16-byte Poly1305 tag (`header ‖ ctr ‖ ciphertext ‖
/// tag ‖ crc`). See [`LinkConfig::secured`].
pub const SEAL_OVERHEAD: usize = 4 + 16;

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
    /// While a SYN goes unanswered, double the gap to the next one, at most
    /// this many times (a ceiling of `syn_interval << syn_backoff`); any
    /// byte received snaps the gap back to `syn_interval`. `0` (every
    /// preset): SYNs stay `syn_interval` apart.
    ///
    /// For a transport with no cable signal, where a board cannot tell that
    /// nobody is listening (a UART behind a USB bridge): its SYNs are binary
    /// that a plain serial monitor prints. The handshake does not wait on it
    /// — a host's SYN is answered at once whatever the gap — so only an
    /// unheard board slows down. A count of doublings rather than a ceiling
    /// in microseconds so the knob costs a link no RAM: it and the link's
    /// own count sit in padding, and a board that leaves it off (the C6 and
    /// S3 on USB) holds exactly the bytes it held before.
    pub syn_backoff: u8,
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
            reliable_channels: (1 << CH_CONTROL) | (1 << CH_PROTO) | (1 << CH_UPDATE),
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
            syn_backoff: 0,
            keepalive: 250_000,
            stall_after: 1_000_000,
            idle_flush: 50_000,
            frame_abandon: 3_000_000,
            escape_ff: true,
        }
    }

    /// BLE NUS: a 15–30 ms connection interval, 244-byte notifications, one
    /// frame per notification (4 header + 236 payload + 4 CRC). A secure
    /// BLE link uses `ble().secured()`, which keeps a sealed frame inside one
    /// notification.
    pub fn ble() -> Self {
        LinkConfig {
            framing: Framing::Datagram,
            max_payload: 236,
            tx_window: 8,
            rx_window: 8,
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

    /// A plain UART through a USB-serial bridge (the classic ESP32's UART0
    /// behind a CH340 at 921,600 baud): `usb()`'s framing on a slower wire
    /// with no flow control.
    ///
    /// Only the windows differ from `usb()`, and both ends use this preset
    /// (the board cuts its buffers further, like the C6's cut of `usb()`):
    ///
    /// - **Framing, CRC, `escape_ff`, payload: `usb()`'s.** The same COBS-FF
    ///   stream, so one sniffer (`LinkSniffer::usb`) reads both links, and
    ///   `0xFF` stays off this wire too: a UART host is still a Web Serial
    ///   page on macOS, the tty path the 0xFF loss was found on. A 256-byte
    ///   payload is ~2.9 ms of line time; the UART's 128-byte FIFO is the
    ///   board's writer's concern (it drains RX between chunks), not the
    ///   frame size's.
    /// - **Windows 4, not 8.** Four frames are ~1 KiB, ~11.6 ms of line time
    ///   each way, which covers a round trip over a bridge whose board side
    ///   is serviced every 1 ms; eight only doubles the transmit and reorder
    ///   buffers, and on a board whose heap is its tightest budget that is
    ///   ~2 KB for nothing. The window is the flow-control ceiling the board
    ///   advertises, so this also bounds what a host has in flight to it.
    /// - **Budgets: `usb()`'s.** A host builds its end from this preset and
    ///   queues each request with `send()`, which refuses a message longer
    ///   than `send_budget`; an upload's ~5.5 KB chunk must fit. The board
    ///   sends its replies external and sets its own small ring.
    /// - **`min_rto`: `usb()`'s 40 ms, for a host.** A host services its end
    ///   promptly. The classic's board raises its own floor to the C6's
    ///   200 ms: its UART is serviced every 1 ms by an interrupt-executor
    ///   task that only moves bytes, but the link itself runs between engine
    ///   ticks on the thread executor, as the C6's does (plan
    ///   `lp2025/2026-09-28-2015-classic-uart-on-lp-link`, D4 and ruling
    ///   DD20; `fw_esp32_common::uart_link::uart_board_link_config`).
    /// - **`frame_abandon`: `usb()`'s 3 s**, for the same reason: a busy
    ///   writer's split frame must not be abandoned and resent in a loop.
    pub fn uart() -> Self {
        LinkConfig {
            tx_window: 4,
            rx_window: 4,
            ..Self::usb()
        }
    }

    /// The same config for a secure link on a transport with a hard frame
    /// size (a BLE notification): `max_payload` keeps meaning *plaintext*,
    /// and a sealed frame is [`SEAL_OVERHEAD`] bytes longer, so this takes
    /// the overhead off the payload to keep each frame inside one
    /// transport packet. A secure BLE link uses `LinkConfig::ble().secured()`.
    /// Stream and WebSocket links need not: their frames have no hard ceiling.
    pub fn secured(self) -> Self {
        LinkConfig {
            max_payload: self.max_payload.saturating_sub(SEAL_OVERHEAD as u16).max(1),
            ..self
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
            ("uart", LinkConfig::uart()),
            ("ble", LinkConfig::ble()),
            ("udp", LinkConfig::udp()),
            ("ws", LinkConfig::ws()),
        ] {
            assert_eq!(cfg.validate(), Ok(()), "{name}");
            assert_eq!(cfg.max_message, MAX_MESSAGE, "{name}");
        }
    }

    /// A secured BLE frame (header, counter, payload, tag, CRC) is exactly
    /// one 244-byte notification, as a plain one is.
    #[test]
    fn a_secured_ble_frame_still_fits_one_notification() {
        let plain = LinkConfig::ble();
        let secure = LinkConfig::ble().secured();
        let wire = |cfg: &LinkConfig, overhead: usize| {
            4 + cfg.max_payload as usize + overhead + cfg.crc.len()
        };
        assert_eq!(wire(&plain, 0), 244);
        assert_eq!(wire(&secure, SEAL_OVERHEAD), 244);
        assert_eq!(secure.validate(), Ok(()));
    }

    /// Tools read a UART link with the USB sniffer (`LinkSniffer::usb`), so
    /// the two presets must frame alike.
    #[test]
    fn uart_frames_like_usb() {
        let (usb, uart) = (LinkConfig::usb(), LinkConfig::uart());
        assert_eq!(uart.framing, usb.framing);
        assert_eq!(uart.crc, usb.crc);
        assert_eq!(uart.escape_ff, usb.escape_ff);
    }
}
