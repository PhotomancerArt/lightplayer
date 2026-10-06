//! One radio link's lp-link configuration: the `ble()` preset, its buffers cut
//! to what the board needs, and its frame size fitted to the connection's ATT
//! MTU.
//!
//! Every lp-link frame on a radio link is **one** ATT operation — one
//! notification board → host, one write host → board (Datagram framing). An
//! ATT value holds `att_mtu − 3` bytes, and a frame is its payload plus the
//! 4-byte header and the 4-byte CRC, so a connection's largest payload is
//! `att_mtu − 3 − 8`, capped at the preset's 180 B (D3,
//! `lp2025/2026-09-28-1445-ble-on-lp-link`). The board advertises that
//! payload in its SYN and lp-link sizes both directions to the smaller of the
//! two ends' values, so the host never needs to know the MTU: it cuts its
//! frames to what the board said. No frame is ever split across an ATT long
//! write (Prepare … Execute), which is why the board no longer reassembles
//! one.

use lp_link::frame::{HEADER_LEN, SYN_LEN};
use lp_link::{LinkConfig, MAX_MESSAGE as LINK_MAX_MESSAGE};

/// The ATT header an ATT value leaves room for: opcode and handle.
const ATT_VALUE_OVERHEAD: usize = 3;

/// The largest message a radio link sends or takes: the static frame buffer's
/// size, as on the USB link (one largest reply, serialized once, read out of
/// that buffer by the link — [`lp_link::Link::send_external`]).
pub const RADIO_MAX_MESSAGE: usize = crate::serial::server_msg::SERVER_MSG_JSON_BUFFER_SIZE;

/// Replies no longer than this are copied into the link's own send ring
/// (`Link::send`), so the shared frame buffer is free again at once; longer
/// ones stay in the frame buffer as an external message until the link has
/// cut them into frames. A hello, a heartbeat, a login exchange, a
/// `SetEncoding` answer and most small replies fit.
pub const SMALL_REPLY_BYTES: usize = 1024;

/// Messages the send ring may hold queued (small replies only).
const SEND_QUEUE: usize = 4;

/// A reassembly buffer above this is released once its request is delivered:
/// a large upload grows it once, and it must not stay for the link's life.
const KEEP_REASSEMBLY: usize = 1024;

/// Log datagrams: none travel on a radio link. The board's log ring has one
/// reader, the USB link task, so a radio link's datagram queue is never used;
/// lp-link wants one slot.
const DATAGRAM_QUEUE: usize = 1;

/// The connection's ATT MTU leaves no room for a SYN (4 header + 12 body + 4
/// CRC = 20 B, which needs an ATT MTU of at least 23 — the Bluetooth
/// minimum): the handshake could never complete, so the link is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MtuTooSmall {
    pub att_mtu: u16,
}

/// The largest lp-link payload one ATT value holds on a connection whose ATT
/// MTU is `att_mtu`, capped at the preset's; `Err` when not even a SYN fits.
///
/// A real, if rare, failure mode (D5): every central measured so far
/// negotiates far above the floor (macOS and the board 247, iOS 185), and a
/// central that never exchanges MTUs sits at the Bluetooth minimum of 23,
/// which still carries 12-byte payloads — slowly, but whole. Below that no
/// frame can ever be carried, and the link is refused rather than opened.
pub fn radio_max_payload(att_mtu: u16) -> Result<u16, MtuTooSmall> {
    let value = usize::from(att_mtu).saturating_sub(ATT_VALUE_OVERHEAD);
    let overhead = HEADER_LEN + LinkConfig::ble().crc.len();
    if value < overhead + SYN_LEN {
        return Err(MtuTooSmall { att_mtu });
    }
    let payload = (value - overhead).min(usize::from(LinkConfig::ble().max_payload));
    Ok(payload as u16)
}

/// The board's configuration for one radio link on a connection whose ATT MTU
/// is `att_mtu`.
///
/// Every buffer is allocated in `Link::new` for the link's life, up to two of
/// them at once ([`super::RADIO_LINK_SLOTS`]), on a heap BLE already makes
/// tight. So, as on the USB link (`UsbLinkShared::config`): replies of more
/// than [`SMALL_REPLY_BYTES`] are not copied into the link at all — the mux
/// serializes each into the static frame buffer and queues it as an external
/// message, whose fragments the link cuts from there into its transmit window.
/// The send ring holds one transmit window plus one small reply; the receive
/// side is lazy (grows with traffic, capped at one largest request, and gives
/// a large reassembly buffer back once its request is delivered); and there
/// is one datagram slot, since no log records travel on a radio link.
pub fn radio_link_config(att_mtu: u16) -> Result<LinkConfig, MtuTooSmall> {
    let max_payload = radio_max_payload(att_mtu)?;
    let mut cfg = LinkConfig::ble();
    cfg.max_payload = max_payload;
    cfg.max_message = RADIO_MAX_MESSAGE;
    // Plus the inbox's 64 B queueing charge (`LinkConfig::validate`).
    cfg.rx_budget = RADIO_MAX_MESSAGE + 64;
    cfg.send_budget = usize::from(cfg.tx_window) * usize::from(max_payload) + SMALL_REPLY_BYTES;
    cfg.send_queue = SEND_QUEUE;
    cfg.keep_reassembly = KEEP_REASSEMBLY;
    cfg.datagram_queue = DATAGRAM_QUEUE;
    Ok(cfg)
}

// The wire's message budget fits the link's: lp-base cannot depend on
// lp-core, so the edge that joins them asserts it (lp-link's README,
// "Message budget").
const _: () = assert!(RADIO_MAX_MESSAGE <= LINK_MAX_MESSAGE);

#[cfg(test)]
mod tests {
    use super::*;
    use lp_link::{Link, SelectiveRepeat};

    extern crate std;

    #[test]
    fn a_frame_always_fits_one_att_value() {
        for att_mtu in [23u16, 31, 64, 185, 191, 247, 517] {
            let payload = usize::from(radio_max_payload(att_mtu).unwrap());
            let frame = HEADER_LEN + payload + 4;
            assert!(
                frame <= usize::from(att_mtu) - 3,
                "mtu {att_mtu}: {frame} B frame"
            );
            assert!(payload <= 180, "mtu {att_mtu}: capped at the preset");
        }
        assert_eq!(radio_max_payload(247), Ok(180), "macOS and the board");
        assert_eq!(radio_max_payload(185), Ok(174), "iOS: 182 B values");
        assert_eq!(
            radio_max_payload(23),
            Ok(12),
            "the Bluetooth minimum still carries a SYN"
        );
    }

    #[test]
    fn a_connection_below_the_bluetooth_minimum_is_refused() {
        assert_eq!(radio_max_payload(22), Err(MtuTooSmall { att_mtu: 22 }));
        assert_eq!(radio_max_payload(0), Err(MtuTooSmall { att_mtu: 0 }));
        assert!(radio_link_config(22).is_err());
    }

    /// The board's cut holds together, carries the largest reply, and costs
    /// well under the preset. `--nocapture` prints the figures (host, 64-bit:
    /// a 32-bit target's descriptors and event queue are about half).
    #[test]
    fn the_board_config_holds_one_largest_reply_and_costs_less_than_the_preset() {
        let cfg = radio_link_config(247).unwrap();
        assert_eq!(cfg.validate(), Ok(()));
        assert!(cfg.max_message >= lpc_wire::PROJECT_READ_FRAME_SERIAL_BUFFER_BYTES);
        assert!(
            cfg.send_budget
                >= cfg.tx_window as usize * cfg.max_payload as usize + SMALL_REPLY_BYTES,
            "the ring holds a full window plus one small reply"
        );
        let board = Link::<SelectiveRepeat>::new(cfg.clone(), 1).ram_bytes();
        let preset = Link::<SelectiveRepeat>::new(LinkConfig::ble(), 1).ram_bytes();
        let bound = Link::<SelectiveRepeat>::ram_bound(&cfg);
        std::println!(
            "radio link RAM at rest: board {board} B, ble() preset {preset} B; board bound {bound} B"
        );
        assert!(board * 2 < preset, "board {board} B vs preset {preset} B");
    }
}
