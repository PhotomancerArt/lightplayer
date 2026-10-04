//! The UART link task's loop: the one owner of the classic's [`Link`]'s
//! timers and frames — on a priority-1 esp-rtos thread of its own
//! (`fw-esp32v3`'s `io_thread`, the default), or on the main thread executor
//! beside the engine (without `io-thread`).
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
//!    or room for a frame), the transport's doorbell, or a log record
//!    landing ([`crate::log_ring_logger::ring_on_record`]) — rung the same
//!    way the C6/S3's USB loop does it
//!    (`crate::usb_link::usb_link_task::run_usb_link`). Nothing wakes the
//!    task on a cadence of its own: with nothing to do it sleeps until the
//!    link's own timers (SYN every 100 ms without a host, keepalive every
//!    250 ms with one) or [`IDLE_BACKSTOP_US`];
//! 5. on a thread of its own, hold the next pass until the chip's
//!    [`PassPacing`] says it may run: every pass preempts the render there,
//!    and on the classic's silicon a preemption costs the render several
//!    times what the pass does (`super::uart_link_pass_pacing`).
//!
//! **Where it runs decides how promptly it runs.** On its own thread
//! (`io-thread`, pinned to core 0 at priority 1, above the main task's 0) a
//! wake — bytes from the I/O task, room in the TX pipe, the doorbell, a timer
//! — preempts the render at once, so ACKs, resends, transfers and the log
//! pump keep the I/O task's ~1 ms cadence while a frame renders; the link is
//! then shared across two threads, under the lock the chip injects
//! ([`UartLinkShared::leak_locked`], and its short-closure rule). Without the
//! thread this task shares the main executor with the engine and runs only
//! between engine ticks (41–114 ms on a dome-scale project). The board's
//! resend floor is sized for the second case, not for the I/O task's 1 ms
//! cadence (`uart_link_config`'s `MIN_RTO_US`). Liveness is the link's own
//! (`Up`/`Reset`/`is_stalled`): a UART has no cable signal, and there is no
//! connection monitor to replace.
//!
//! Waking on events and not on a 10 ms cadence matters because this task
//! has a thread of its own (`io-thread`): every pass preempts the render,
//! and a pass that finds nothing to do still costs something. The C6's USB
//! loop made the same change in M1 (`lp2025/2026-10-01-1200-io-thread-spike`);
//! P2 of `lp2025/2026-10-02-1918-io-thread-other-boards` ported it here
//! ahead of the thread, which P4 of that plan added.

use core::cell::Cell;

use critical_section::Mutex;
use embassy_futures::select::select3;
use embassy_time::{Instant, Timer};
use lp_link::{Link, LinkState, Micros, SelectiveRepeat};

use super::uart_link_counters::{self, EdgeCounters};
use super::uart_link_pass_pacing::PassPacing;
use super::uart_link_pipes::{self, MAX_FRAME_BYTES};
use super::uart_link_shared::UartLinkShared;

/// The longest the task sleeps with nothing to do. A backstop only: log
/// records, queued replies and the I/O task's news (bytes arrived, room for
/// a frame) all ring or wake sooner, and the link's own timers come sooner
/// whenever a host is there or being looked for. Was `IDLE_CAP_US = 10_000`
/// (a true 10 ms poll); event-driven since P2 of
/// `lp2025/2026-10-02-1918-io-thread-other-boards`, mirroring the C6/S3 USB
/// loop's `IDLE_BACKSTOP_US`.
pub const IDLE_BACKSTOP_US: Micros = 250_000;
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

/// Run the host link for ever. Spawn it on a thread executor — the link
/// thread's, or the main one — never on the I/O task's interrupt executor.
/// `pacing` is how far apart its passes must start: the link thread's
/// [`PassPacing::CLASSIC_LINK_THREAD`], or [`PassPacing::EVERY_EVENT`] on the
/// main executor, where it only runs between frames anyway.
pub async fn run_uart_link(shared: &'static UartLinkShared, pacing: PassPacing) -> ! {
    let mut buf = [0u8; RX_CHUNK];
    let mut frame = [0u8; MAX_FRAME_BYTES];
    let mut drain_asked_at: Option<Micros> = None;
    let mut io_live_said = false;
    let mut edge_seen = EdgeCounters::default();
    crate::log_ring_logger::ring_on_record(shared.doorbell_signal());

    loop {
        let pass_started = now_us();
        feed_rx(shared, &mut buf);

        let now = now_us();
        let logs_moved = shared.with_link(|link| {
            let moved = crate::log_ring_logger::pump(link, now, LOG_RECORDS_PER_PASS);
            uart_link_counters::note_stalled(link.is_stalled(now));
            moved
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

        // A burst longer than one pass's records: go round again while the
        // link keeps taking them (the datagram queue's own room gates it).
        // A pass that moved none waits for the event that makes room (an ACK
        // arriving, a timer) or a new record — the C6/S3 USB loop's
        // `log_backlog` rule.
        let log_backlog = logs_moved > 0 && crate::log_ring_logger::has_records();
        let wake = if log_backlog {
            now_us()
        } else {
            wake_at(shared, IDLE_BACKSTOP_US)
        };
        select3(
            uart_link_pipes::wake(),
            Timer::at(Instant::from_micros(wake)),
            shared.doorbell(),
        )
        .await;
        // Woken; on a thread of its own, the pass waits out its interval.
        if let Some(hold) = pacing.hold_until(pass_started, now_us()) {
            Timer::at(Instant::from_micros(hold)).await;
        }
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
