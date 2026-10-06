//! One LAN link's lp-link configuration: the `ws()` preset, cut to what a
//! board can hold.
//!
//! A LAN link is one WebSocket on `ws://<board>/link`; each binary message
//! is one lp-link frame (Datagram framing, as on Bluetooth). The host builds
//! its end from [`LinkConfig::ws`] unchanged (lp-cli's `lan:`, Studio's
//! `?lan=`); the board cuts its own buffers, as the C6 cuts `usb()`, and
//! advertises its payload in its SYN so both ends size frames to the
//! smaller.
//!
//! - **ARQ: selective repeat, as on every other link.** The preset's doc
//!   pairs `ws()` with lp-link's no-ARQ variant, since TCP already
//!   retransmits. The board runs the one ARQ its other links run (and the
//!   hosts' `WireLinkPort` already speaks): over TCP nothing is ever lost,
//!   so it never resends, and the image carries no second ARQ
//!   (plan deviation, recorded in the PR: no-ARQ was ~8 KB of code).
//! - **Payload 1 KB** (the preset's): one WebSocket message, well inside
//!   one TCP segment pair; a project upload's 5.5 KB chunk is six frames.
//! - **Windows 4**: 4 KB in flight each way covers a LAN round trip at the
//!   board's frame rate, and the send ring is allocated for the link's life
//!   (the preset's 16 would be 16 KB per link).
//! - **Budgets: the radio links'.** A long reply stays in the shared frame
//!   buffer as an external message (never a second 16 KiB copy per link);
//!   the receive side is lazy, capped at one largest request.
//! - **Timers: the preset's** (300 ms first RTO, 1 s keepalive). With a
//!   reliable transport they only matter if the peer goes quiet.
//!
//! Secure is required: the slot opens it with `Link::new_secure` as the
//! Noise responder (`RadioLinkSlot::open_lan`).

use lp_link::LinkConfig;

use super::radio_link_config::{RADIO_MAX_MESSAGE, SMALL_REPLY_BYTES};

/// The board's frames in flight each way.
const LAN_WINDOW: u8 = 4;
/// Messages the send ring may hold queued (small replies only).
const SEND_QUEUE: usize = 4;
/// A reassembly buffer above this is released once its request is
/// delivered.
const KEEP_REASSEMBLY: usize = 1024;

/// The board's configuration for one LAN link.
pub fn lan_link_config() -> LinkConfig {
    let mut cfg = LinkConfig::ws();
    cfg.tx_window = LAN_WINDOW;
    cfg.rx_window = LAN_WINDOW;
    cfg.max_message = RADIO_MAX_MESSAGE;
    // Plus the inbox's 64 B queueing charge (`LinkConfig::validate`).
    cfg.rx_budget = RADIO_MAX_MESSAGE + 64;
    cfg.send_budget = usize::from(cfg.tx_window) * usize::from(cfg.max_payload) + SMALL_REPLY_BYTES;
    cfg.send_queue = SEND_QUEUE;
    cfg.keep_reassembly = KEEP_REASSEMBLY;
    // No log records travel on a LAN link (one reader: the USB link task).
    cfg.datagram_queue = 1;
    cfg
}

/// The largest frame a LAN link sends or takes on the wire: its payload,
/// the frame header and CRC, and the secure seal, with room to spare. The
/// LAN endpoint sizes its WebSocket buffers from it.
pub const LAN_MAX_FRAME: usize = 1024 + 64;

#[cfg(test)]
mod tests {
    use super::*;
    use lp_link::{Link, SelectiveRepeat};

    extern crate std;

    #[test]
    fn the_board_config_holds_one_largest_reply_and_costs_less_than_the_preset() {
        let cfg = lan_link_config();
        assert_eq!(cfg.validate(), Ok(()));
        assert!(cfg.max_message >= lpc_wire::PROJECT_READ_FRAME_SERIAL_BUFFER_BYTES);
        let board = Link::<SelectiveRepeat>::ram_bound_secure(&cfg);
        let preset = Link::<SelectiveRepeat>::ram_bound_secure(&LinkConfig::ws());
        std::println!("LAN link RAM bound: board {board} B, ws() preset {preset} B");
        assert!(board < preset, "board {board} B vs preset {preset} B");
    }

    #[test]
    fn a_frame_fits_the_endpoints_buffers() {
        let cfg = lan_link_config();
        // 4 B header + payload + 4 B CRC + 16 B seal.
        assert!(4 + usize::from(cfg.max_payload) + 4 + 16 <= LAN_MAX_FRAME);
    }
}
