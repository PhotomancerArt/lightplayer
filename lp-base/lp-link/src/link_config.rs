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
    /// closes.
    pub rx_budget: usize,
    /// Bytes `send()` queues (pending + unacknowledged) before `Full`.
    pub send_budget: usize,
    /// Longest reliable message.
    pub max_message: usize,
    /// Best-effort messages queued before `Full`.
    pub datagram_queue: usize,
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
    /// Flush a partial frame or unterminated text after this much quiet.
    pub idle_flush: Micros,
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
            rx_budget: 16 * 1024,
            send_budget: 40 * 1024,
            max_message: 20 * 1024,
            datagram_queue: 32,
            reliable_channels: (1 << CH_CONTROL) | (1 << CH_PROTO),
            ack_delay: 1_000,
            ack_every: 2,
            reorder_threshold: 1,
            initial_rto: 50_000,
            min_rto: 10_000,
            max_rto: 1_000_000,
            max_retries: 20,
            syn_interval: 100_000,
            keepalive: 250_000,
            stall_after: 1_000_000,
            idle_flush: 50_000,
        }
    }

    /// BLE NUS: a 15–30 ms connection interval, 244-byte notifications, one
    /// frame per notification (4 header + 236 payload + 4 CRC).
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

    pub fn is_reliable(&self, channel: u8) -> bool {
        channel < 8 && self.reliable_channels & (1 << channel) != 0
    }
}
