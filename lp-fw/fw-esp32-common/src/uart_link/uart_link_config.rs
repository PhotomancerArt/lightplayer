//! The classic's link configuration: lp-link's UART preset, its buffers cut
//! to what the board needs.
//!
//! The same shape as the C6's cut of the USB preset
//! (`crate::usb_link::UsbLinkShared::config`), smaller again: the classic's
//! heap, not its flash, is its binding budget. Every buffer here is allocated
//! in `Link::new` for the link's life, once per boot, before any project
//! loads — so its cost is paid out of the headroom every later load and read
//! is gated on (`lpa_server`'s 64 KiB load gate and 32 KiB read gate). The
//! bottom-of-file test measures it and holds it against the headroom the
//! shipped image was measured to have with a project loaded.

use lp_link::LinkConfig;

/// The largest message the board sends or takes: the static frame buffer's
/// size, which bounds every reply it can serialize (the 16 KiB `ProjectRead`
/// frame budget plus its serial margin), and the wire's budget for a request.
#[cfg(feature = "server")]
pub const MAX_MESSAGE: usize = crate::serial::server_msg::SERVER_MSG_JSON_BUFFER_SIZE;
#[cfg(not(feature = "server"))]
pub const MAX_MESSAGE: usize = lp_link::MAX_MESSAGE;

/// Log records the link may hold queued at once: two whole-frame slots
/// (256 B each), half the C6's four. The log ring (a 4 KB static, which on
/// this chip comes out of `.stack`'s residual, not the heap) holds the rest,
/// and a 921,600-baud line drains logs more slowly than USB anyway.
const LOG_DATAGRAMS: usize = 2;

/// Messages the send ring may hold queued. Replies go as external messages
/// (one at a time, from the static frame buffer); the ring carries only the
/// small ones built elsewhere (a dropped reply's error notice). The C6's
/// value.
const SEND_QUEUE: usize = 4;

/// The send ring: a full transmit window of external fragments (4 frames x
/// 256 B, which count against the same budget while a reply is going out)
/// plus one dropped reply's error notice (the USB transport serializes those
/// into 192 B). Half the C6's 2,560 B, for half its window.
const SEND_BUDGET: usize = 1280;

/// A reassembly buffer above this is released once its request is delivered.
/// An upload's chunk grows one to ~5.5 KB (`FILE_SYNC_CHUNK_BYTES`' 4 KiB of
/// data, base64 in its JSON envelope); it must not stay. Half the C6's
/// 1,024 B: every request that is not an upload is well under it.
const KEEP_REASSEMBLY: usize = 512;

/// The board's link configuration. See the module docs.
///
/// `min_rto` stays at the preset's 40 ms, not the C6's 200 ms: the C6's link
/// task yields to an ~80 ms render tick between packets, and the classic's
/// UART is serviced from an interrupt executor every 1 ms whatever the engine
/// is doing. That holds only if the link itself is driven from there (P2);
/// until an emulated soak and a desk sitting say otherwise it is a
/// hypothesis, not a measurement (plan D4).
pub fn uart_board_link_config() -> LinkConfig {
    let mut cfg = LinkConfig::uart();
    cfg.max_message = MAX_MESSAGE;
    cfg.send_budget = SEND_BUDGET;
    // Plus the inbox's 64 B queueing charge (`LinkConfig::validate`).
    cfg.rx_budget = MAX_MESSAGE + 64;
    cfg.keep_reassembly = KEEP_REASSEMBLY;
    cfg.send_queue = SEND_QUEUE;
    cfg.datagram_queue = LOG_DATAGRAMS;
    cfg
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alloc::vec::Vec;

    use lp_link::{CH_PROTO, Link, LinkEvent, LinkState, SelectiveRepeat};

    use super::*;

    /// The board's cut holds together, carries the largest reply, and costs
    /// well under the preset. (`--nocapture` prints the figures.)
    #[test]
    fn the_board_config_holds_one_largest_reply_and_costs_less_than_the_preset() {
        let cfg = uart_board_link_config();
        assert_eq!(cfg.validate(), Ok(()));
        #[cfg(feature = "server")]
        assert!(cfg.max_message >= lpc_wire::PROJECT_READ_FRAME_SERIAL_BUFFER_BYTES);
        assert!(
            cfg.send_budget >= cfg.tx_window as usize * cfg.max_payload as usize + 192,
            "the ring holds a full window plus a notice"
        );
        let board = Link::<SelectiveRepeat>::new(cfg.clone(), 1).ram_bytes();
        let preset = Link::<SelectiveRepeat>::new(LinkConfig::uart(), 1).ram_bytes();
        let bound = Link::<SelectiveRepeat>::ram_bound(&cfg);
        std::println!(
            "uart link RAM at rest: board {board} B, preset {preset} B; board bound {bound} B"
        );
        assert!(board * 2 < preset, "board {board} B vs preset {preset} B");
    }

    /// What the link holds at its realistic peak: an upload's largest
    /// request mid-reassembly, and a whole reply going out from the frame
    /// buffer. Both give their memory back once done. (`--nocapture` prints
    /// the figures.)
    #[test]
    fn an_upload_chunk_and_a_largest_reply_come_and_go() {
        let cfg = uart_board_link_config();
        let mut board = Link::<SelectiveRepeat>::new(cfg.clone(), 0xB0A2_0001);
        let mut host = Link::<SelectiveRepeat>::new(LinkConfig::uart(), 0x4057_0001);
        let rest = board.ram_bytes();
        let mut now = 0;
        while board.state() != LinkState::Established || host.state() != LinkState::Established {
            step(&mut host, &mut board, None, &mut now);
            assert!(now < 1_000_000, "the handshake completes");
        }
        while board.recv().is_some() {}
        while host.recv().is_some() {}

        // Inbound: a file-sync write chunk (4 KiB of data, base64 in its
        // envelope, ~5.5 KB) — the largest request a client sends in bulk.
        let upload = alloc::vec![b'u'; 5_600];
        host.send(CH_PROTO, &upload).unwrap();
        let mut peak_in = board.ram_bytes();
        let mut got: Option<Vec<u8>> = None;
        while got.is_none() {
            step(&mut host, &mut board, None, &mut now);
            peak_in = peak_in.max(board.ram_bytes());
            while let Some(event) = board.recv() {
                if let LinkEvent::Message { data, .. } = event {
                    got = Some(data);
                }
            }
            assert!(now < 5_000_000, "the upload is delivered");
        }
        assert_eq!(got.as_deref(), Some(&upload[..]));
        drop(got);
        let after_in = board.ram_bytes();

        // Outbound: the largest reply, sent external from a stand-in for the
        // static frame buffer (no copy into the link).
        let frame_buf = alloc::vec![b'r'; MAX_MESSAGE - 64];
        board.send_external(CH_PROTO, frame_buf.len()).unwrap();
        let mut peak_out = board.ram_bytes();
        let mut delivered = false;
        while !delivered {
            step(&mut host, &mut board, Some(&frame_buf), &mut now);
            peak_out = peak_out.max(board.ram_bytes());
            while let Some(event) = host.recv() {
                if let LinkEvent::Message { data, .. } = event {
                    assert_eq!(data.len(), frame_buf.len());
                    delivered = true;
                }
            }
            assert!(now < 20_000_000, "the reply is delivered");
        }
        step(&mut host, &mut board, Some(&frame_buf), &mut now);
        let after_out = board.ram_bytes();

        std::println!(
            "uart link RAM: rest {rest} B; upload peak {peak_in} B, after {after_in} B; \
             reply peak {peak_out} B, after {after_out} B"
        );
        assert!(
            after_in <= rest + KEEP_REASSEMBLY,
            "the upload's buffer is given back"
        );
        assert!(
            peak_out <= rest + KEEP_REASSEMBLY,
            "a reply is not copied into the link"
        );
    }

    /// One millisecond of both ends: each end's frames go to the other.
    fn step(
        host: &mut Link<SelectiveRepeat>,
        board: &mut Link<SelectiveRepeat>,
        external: Option<&[u8]>,
        now: &mut u64,
    ) {
        while let Some(frame) = host.poll_transmit(*now) {
            let frame = frame.to_vec();
            board.on_bytes(*now, &frame);
        }
        loop {
            let frame = match external {
                Some(buf) => board.poll_transmit_with(*now, &mut |offset, out: &mut [u8]| {
                    out.copy_from_slice(&buf[offset..offset + out.len()]);
                }),
                None => board.poll_transmit(*now),
            };
            let Some(frame) = frame else { break };
            let frame = frame.to_vec();
            host.on_bytes(*now, &frame);
        }
        *now += 1_000;
    }
}
