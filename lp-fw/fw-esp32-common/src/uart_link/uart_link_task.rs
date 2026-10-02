//! The UART link task's loop: the one owner of the classic's [`Link`], on the
//! thread executor.
//!
//! It never touches UART0. The classic's I/O task does, from its interrupt
//! executor every 1 ms (`fw-esp32v3`'s `serial::io_task`), and hands bytes
//! across [`super::uart_link_pipes`]; this task turns them into link events
//! and the link's frames back into bytes. That split is ruling DD20 of plan
//! `classic-uart-on-lp-link`: the I/O task stays a byte shuttle under the
//! executor-isolation rules (no embassy-time, no logging, ISR-scale stack —
//! `docs/adr/2026-08-25-classic-uart-io-task-executor-isolation.md`), and the
//! link, its timers and its logs live here, where all three are allowed.
//!
//! The shape is the C6's (`crate::usb_link::usb_link_task`), minus the
//! writing itself:
//!
//! 1. feed the link what the RX pipe holds;
//! 2. move log records onto the log channel;
//! 3. queue whole frames into the TX pipe while it has room for a largest
//!    one, **feeding the link RX between frames** — a frame is only overdue
//!    if its ACK has not *arrived*, not if it waits unread in a pipe;
//! 4. sleep until the link's next timer, the I/O task's news (bytes arrived,
//!    or room for a frame), or the transport's doorbell.
//!
//! Because this task shares the thread executor with the engine, it runs
//! only between engine ticks (41–114 ms on a dome-scale project), and the
//! board's resend floor is sized for that, not for the I/O task's 1 ms
//! cadence (`uart_link_config`'s `MIN_RTO_US`). Liveness is the link's own
//! (`Up`/`Reset`/`is_stalled`): a UART has no cable signal, and there is no
//! connection monitor to replace.

use core::cell::Cell;

use critical_section::Mutex;
use embassy_futures::select::select3;
use embassy_time::{Instant, Timer};
use lp_link::{Link, LinkState, Micros, SelectiveRepeat};

use super::uart_link_counters::{self, EdgeCounters};
use super::uart_link_pipes::{self, MAX_FRAME_BYTES};
use super::uart_link_shared::UartLinkShared;

/// The longest the task sleeps with nothing to do (the log ring's cadence).
pub const IDLE_CAP_US: Micros = 10_000;
/// Log records moved onto the log channel per pass: the board's datagram
/// queue (`uart_link_config`'s two slots). Each is popped under its own
/// short critical section (see [`crate::log_ring_logger::pump`]).
const LOG_RECORDS_PER_PASS: usize = 2;
/// Bytes taken from the RX pipe per read.
const RX_CHUNK: usize = 128;
/// Longest a terminal action ([`when_drained`]) waits for the host to
/// acknowledge what the link holds.
const DRAIN_LIMIT_US: Micros = 1_000_000;

static WHEN_DRAINED: Mutex<Cell<Option<fn() -> !>>> = Mutex::new(Cell::new(None));

/// Run `action` (a reset: something that ends this boot) from the link task
/// once the host has everything the link holds — the answer to the request
/// that asked for it above all.
///
/// A reply the server has sent is only *queued* on the link: resetting at
/// once (what the `M!` path did, once the bytes were written) would take the
/// answer and the last log lines down with the board, and the host would see
/// a session reset instead of its reply. So the platform hook asks, and the
/// link task acts when the link is idle (everything acknowledged), when no
/// host is up to acknowledge anything, or after [`DRAIN_LIMIT_US`] at most. A
/// second request replaces the first. The C6's
/// `crate::usb_link::when_drained`, for this link.
pub fn when_drained(action: fn() -> !) {
    critical_section::with(|cs| WHEN_DRAINED.borrow(cs).set(Some(action)));
}

/// Run the host link for ever. Spawn it on the thread executor.
pub async fn run_uart_link(shared: &'static UartLinkShared) -> ! {
    let mut buf = [0u8; RX_CHUNK];
    let mut frame = [0u8; MAX_FRAME_BYTES];
    let mut drain_asked_at: Option<Micros> = None;
    let mut io_live_said = false;
    let mut edge_seen = EdgeCounters::default();

    loop {
        feed_rx(shared, &mut buf);

        let now = now_us();
        shared.with_link(|link| {
            crate::log_ring_logger::pump(link, now, LOG_RECORDS_PER_PASS);
            uart_link_counters::note_stalled(link.is_stalled(now));
        });

        // Whole frames, while the TX pipe has room for a largest one. When it
        // has not, the I/O task wakes this task once it has drained enough.
        while uart_link_pipes::room_for_frame() {
            let next = shared.with_link(|link| {
                link.poll_transmit_with(now_us(), &mut external_source)
                    .map(|f| {
                        let n = f.len().min(frame.len());
                        frame[..n].copy_from_slice(&f[..n]);
                        (n, f.len())
                    })
            });
            let Some((n, len)) = next else { break };
            // A frame longer than the link's own largest cannot happen; one
            // that did would be cut, and the link resends what matters.
            if n == len {
                uart_link_pipes::put_frame(&frame[..n]);
            }
            feed_rx(shared, &mut buf);
        }

        shared.with_link(|link| uart_link_counters::publish(link.counters()));
        say_edge_news(&mut edge_seen, &mut io_live_said);

        if let Some(action) = critical_section::with(|cs| WHEN_DRAINED.borrow(cs).get()) {
            let now = now_us();
            let asked = *drain_asked_at.get_or_insert(now);
            let drained =
                shared.with_link(|link| link.state() != LinkState::Established || link.is_idle());
            if drained || now.saturating_sub(asked) >= DRAIN_LIMIT_US {
                action();
            }
        }

        let wake = wake_at(shared, IDLE_CAP_US);
        select3(
            uart_link_pipes::wake(),
            Timer::at(Instant::from_micros(wake)),
            shared.doorbell(),
        )
        .await;
    }
}

/// Where the link reads an external message's bytes: the static frame buffer
/// the transport serialized the reply into, which it keeps unchanged while
/// `Link::external_in_flight` holds (see [`super::uart_link_transport`]).
pub(crate) fn external_source(offset: usize, out: &mut [u8]) {
    #[cfg(feature = "server")]
    {
        let bytes = crate::serial::server_msg::frame_bytes(offset + out.len());
        out.copy_from_slice(&bytes[offset..]);
    }
    // No server, no frame buffer: nothing sends an external message.
    #[cfg(not(feature = "server"))]
    {
        let _ = offset;
        out.fill(0);
    }
}

/// The device clock, in the link's unit.
pub fn now_us() -> Micros {
    Instant::now().as_micros()
}

/// Feed the link whatever the RX pipe holds, without waiting.
fn feed_rx(shared: &UartLinkShared, buf: &mut [u8; RX_CHUNK]) {
    loop {
        let n = uart_link_pipes::take_rx(buf);
        if n == 0 {
            return;
        }
        let t = now_us();
        shared.with_link(|link| link.on_bytes(t, &buf[..n]));
    }
}

/// When the link next needs a pass for a timer, capped so the log ring is
/// pumped at least every `cap_us`.
fn wake_at(shared: &UartLinkShared, cap_us: Micros) -> Micros {
    let now = now_us();
    shared.with_link(|link: &mut Link<SelectiveRepeat>| {
        link.poll_timeout()
            .unwrap_or(Micros::MAX)
            .min(now + cap_us)
            .max(now)
    })
}

/// Say, from here, what the I/O task may not say from its interrupt
/// executor: that it is running at all (once), and each increase in what the
/// UART edge lost. Those losses are recovered by the link (a damaged or
/// missing frame is resent), so they are warnings, not errors.
fn say_edge_news(seen: &mut EdgeCounters, io_live_said: &mut bool) {
    if !*io_live_said && uart_link_pipes::io_passes() > 0 {
        *io_live_said = true;
        log::info!("[uart_link] I/O task live: pacer ticks flowing (swi2 executor)");
    }
    let now = uart_link_counters::edge();
    if now.rx_errors > seen.rx_errors {
        log::warn!(
            "[uart_link] UART RX errors: +{} ({} since boot); the link resends what they cost",
            now.rx_errors - seen.rx_errors,
            now.rx_errors
        );
    }
    if now.rx_pipe_overflow_bytes > seen.rx_pipe_overflow_bytes {
        log::warn!(
            "[uart_link] RX pipe full: {} B dropped ({} since boot); the link resends them",
            now.rx_pipe_overflow_bytes - seen.rx_pipe_overflow_bytes,
            now.rx_pipe_overflow_bytes
        );
    }
    if now.write_failures > seen.write_failures {
        log::warn!(
            "[uart_link] UART writes failed: +{} ({} since boot); the link resends them",
            now.write_failures - seen.write_failures,
            now.write_failures
        );
    }
    *seen = now;
}
