//! The bytes between the classic's UART I/O task and its link task.
//!
//! The two run on different executors, and that is the design, not an
//! accident (`docs/adr/2026-08-25-classic-uart-io-task-executor-isolation.md`):
//!
//! - the **I/O task** (`fw-esp32v3`'s `serial::io_task`) owns UART0 and polls
//!   it every 1 ms from an esp-rtos *interrupt* executor (swi2), whatever the
//!   engine is doing. It only moves bytes: the RX FIFO into [`io_received`],
//!   and what [`io_take_tx`] hands it out through the chunked writer;
//! - the **link task** ([`super::uart_link_task`]) owns the
//!   [`Link`](lp_link::Link) on the *thread* executor, beside the server
//!   transport that shares it (plan `classic-uart-on-lp-link`, ruling DD20):
//!   it feeds the link what arrived and queues whole frames to go.
//!
//! The `Link` itself never crosses: a `RefCell` borrowed from a task the swi2
//! executor can preempt would be a `BorrowMutError` waiting to happen, and a
//! lock around it would mask interrupts for as long as the link works. Only
//! bytes cross, through two fixed pipes whose lock (a critical section) is
//! held for one short copy, and one wake signal back to the link task. All
//! of it is safe from interrupt context: no allocation, no logging, no
//! embassy-time.
//!
//! **One writer per unit.** Everything UART0 carries after boot goes through
//! the TX pipe, and the link task puts each frame into it whole
//! ([`put_frame`]), or not at all. So a frame is never split by another
//! writer's bytes: the concurrent-writer defect
//! (`docs/defects/2026-08-02-serial-line-interleaving.md`) has no second
//! writer left to interleave with, apart from boot text written before any
//! host can have a session, and a panic's text, which follows lp-link's
//! `0xFF` text mark. A diagnostic too long for a log record queues whole
//! text lines the same way ([`put_text_line`]).
//!
//! Sizes: the host can have at most its window of frames in flight to the
//! board (the board advertises 4, `LinkConfig::uart`), ~1.1 KB of JSON
//! frames, so the RX pipe holds that with room for the ACKs beside it even
//! while an engine tick keeps the link task away for ~100 ms. The TX pipe
//! holds the board's own window of frames: a frame is queued only when a
//! worst-case one ([`MAX_FRAME_BYTES`], every byte escaped) would fit, and a
//! typical one is half that. A pipe that overflows loses bytes, which the
//! link resends; the loss is counted (`uart_link_counters`).

use core::sync::atomic::{AtomicU32, Ordering::Relaxed};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::pipe::Pipe;
use embassy_sync::signal::Signal;
use lp_link::CrcKind;

use super::uart_link_counters;

/// Bytes from the RX FIFO waiting for the link task: the host's window of
/// frames (4 x ~270 B; a host sends JSON, which escapes almost nothing) plus
/// its ACKs, with margin.
pub const RX_PIPE_BYTES: usize = 1536;

/// Frames waiting for the I/O task to write: three typical frames (~270 B)
/// and room still for a worst-case fourth, so a full window of typical
/// frames queues in one pass.
pub const TX_PIPE_BYTES: usize = 1536;

/// The largest frame the board's link writes, delimiters included: a
/// 256-byte payload (`LinkConfig::uart`'s `max_payload`), its header and
/// CRC-32C, COBS-FF-encoded with every byte escaped (533 B; a typical frame
/// is ~270). The link task queues a frame only when the TX pipe has this much
/// room.
pub const MAX_FRAME_BYTES: usize = lp_link::frame::max_encoded_len(256, CrcKind::Crc32c);

static RX: Pipe<CriticalSectionRawMutex, RX_PIPE_BYTES> = Pipe::new();
static TX: Pipe<CriticalSectionRawMutex, TX_PIPE_BYTES> = Pipe::new();

/// Wakes the link task: bytes arrived, or the TX pipe has room for a frame.
static LINK_WAKE: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// I/O task passes since boot: the link task's evidence that the pacer ticks
/// arrive (the I/O task itself may not log).
static IO_PASSES: AtomicU32 = AtomicU32::new(0);

/// **I/O task.** Bytes the RX FIFO gave up: into the RX pipe for the link
/// task. What does not fit is dropped and counted; the link resends it.
pub fn io_received(bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    let taken = write_all(&RX, bytes);
    if taken < bytes.len() {
        uart_link_counters::note_rx_pipe_overflow(bytes.len() - taken);
    }
    LINK_WAKE.signal(());
}

/// **I/O task.** The next bytes to write, at most `out.len()`; 0 when there
/// are none. Wakes the link task once there is room for another frame.
pub fn io_take_tx(out: &mut [u8]) -> usize {
    let n = TX.try_read(out).unwrap_or(0);
    if n > 0 && TX.free_capacity() >= MAX_FRAME_BYTES {
        LINK_WAKE.signal(());
    }
    n
}

/// **I/O task.** One pass of its loop happened.
pub fn io_pass() {
    IO_PASSES.fetch_add(1, Relaxed);
}

/// Link task: whatever arrived, at most `out.len()` of it.
pub(crate) fn take_rx(out: &mut [u8]) -> usize {
    RX.try_read(out).unwrap_or(0)
}

/// Link task: room for one more largest frame.
pub(crate) fn room_for_frame() -> bool {
    TX.free_capacity() >= MAX_FRAME_BYTES
}

/// Link task: queue one whole frame. Call only after [`room_for_frame`]; the
/// link task is the TX pipe's only writer and the I/O task only takes from
/// it, so the room cannot shrink in between. `false` (and nothing queued) if
/// it somehow did.
pub(crate) fn put_frame(frame: &[u8]) -> bool {
    if TX.free_capacity() < frame.len() {
        return false;
    }
    write_all(&TX, frame) == frame.len()
}

/// **Thread executor only** (the link task's): one whole line of console
/// text, queued between frames, for a diagnostic whose line is longer than a
/// log record holds (`lp_link::log_ring::MAX_RECORD_TEXT`, 200 B) and whose
/// readers parse it by position — the `ws281x_telemetry` build's `[WS281X]`
/// lines. The host sees it as text (`LinkEvent::Text`), as it sees boot
/// text.
///
/// Queued whole or not at all (`false`: no room, or a byte a text line may
/// not hold — `0x00` opens a frame, `0xFF` is the text mark). It can never
/// land inside a frame: the link task queues each frame whole from the same
/// executor, and neither awaits in the middle. Not for anything a log line
/// can carry: log records are what a host's console and recorder expect.
pub fn put_text_line(line: &[u8]) -> bool {
    if line
        .iter()
        .any(|&b| b == 0x00 || b == lp_link::deframer::TEXT_MARK)
    {
        return false;
    }
    if TX.free_capacity() < line.len() {
        return false;
    }
    write_all(&TX, line) == line.len()
}

/// Link task: sleep until the I/O task has news.
pub(crate) async fn wake() {
    LINK_WAKE.wait().await;
}

/// Link task: how many passes the I/O task has made.
pub(crate) fn io_passes() -> u32 {
    IO_PASSES.load(Relaxed)
}

/// As much of `bytes` as `pipe` has room for; how much that was.
///
/// A loop, not one `try_write`: `Pipe::try_write` copies only up to the end
/// of its ring, so at the wrap it takes part of what fits and says so. Taken
/// as "full", that dropped bytes from the middle of a frame with room to
/// spare — the emulated classic showed it as a steady trickle of 4-byte RX
/// drops (and damaged frames) under an idle host's keepalives.
fn write_all<const N: usize>(pipe: &Pipe<CriticalSectionRawMutex, N>, bytes: &[u8]) -> usize {
    let mut done = 0;
    while done < bytes.len() {
        match pipe.try_write(&bytes[done..]) {
            Ok(n) if n > 0 => done += n,
            _ => break,
        }
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pipes hold what their docs say they hold.
    #[test]
    fn the_pipes_hold_a_window_of_frames() {
        let cfg = super::super::uart_board_link_config();
        assert_eq!(cfg.max_payload, 256, "MAX_FRAME_BYTES assumes it");
        assert_eq!(cfg.crc, CrcKind::Crc32c, "MAX_FRAME_BYTES assumes it");
        // A frame with nothing to escape: plain COBS's bound, delimiters
        // included.
        let typical = 2 + lp_link::cobs::max_encoded_len(
            lp_link::frame::HEADER_LEN + cfg.max_payload as usize + cfg.crc.len(),
        );
        let window = cfg.tx_window as usize;
        assert!(
            TX_PIPE_BYTES >= (window - 1) * typical + MAX_FRAME_BYTES,
            "the board's window of typical frames queues in one pass"
        );
        assert!(
            RX_PIPE_BYTES >= cfg.rx_window as usize * typical + 256,
            "the host's window, and its ACKs, fit the RX pipe"
        );
    }

    /// Frames and text lines go through the TX pipe whole — across its wrap
    /// too — and a line that could be read as a frame or a text mark does not
    /// go out at all. (The one test that touches the TX pipe.)
    #[test]
    fn frames_and_text_lines_are_queued_whole_across_the_wrap() {
        let mut out = [0u8; TX_PIPE_BYTES];
        assert!(!put_text_line(b"[WS281X] a\x00b\r\n"));
        assert!(!put_text_line(b"[WS281X] a\xffb\r\n"));
        assert!(put_text_line(b"[WS281X] t_ms=1 ch=0\r\n"));
        let n = io_take_tx(&mut out);
        assert_eq!(&out[..n], b"[WS281X] t_ms=1 ch=0\r\n");
        assert_eq!(io_take_tx(&mut out), 0, "nothing else was queued");

        // Walk the ring round so the next frame straddles its end.
        for step in 0..8 {
            let frame = alloc::vec![0x40 + step; 300];
            assert!(room_for_frame());
            assert!(put_frame(&frame), "frame {step} queued whole");
            let mut got = alloc::vec::Vec::new();
            loop {
                let n = io_take_tx(&mut out);
                if n == 0 {
                    break;
                }
                got.extend_from_slice(&out[..n]);
            }
            assert_eq!(got, frame, "frame {step} came out whole");
        }
    }

    /// Bytes from the RX FIFO reach the link task whole across the RX pipe's
    /// wrap. (The one test that touches the RX pipe.)
    #[test]
    fn received_bytes_arrive_whole_across_the_wrap() {
        let mut out = [0u8; RX_PIPE_BYTES];
        for step in 0..8u8 {
            let bytes = alloc::vec![step; 400];
            io_received(&bytes);
            let mut got = alloc::vec::Vec::new();
            loop {
                let n = take_rx(&mut out);
                if n == 0 {
                    break;
                }
                got.extend_from_slice(&out[..n]);
            }
            assert_eq!(got, bytes, "step {step}");
        }
        assert_eq!(
            super::super::uart_link_counters::edge().rx_pipe_overflow_bytes,
            0
        );
    }
}
