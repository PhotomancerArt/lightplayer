//! The USB link task's loop: the one owner of the USB-Serial-JTAG halves.
//!
//! A byte stream, so frames go out COBS-FF-encoded between `0x00` delimiters
//! ([`lp_link::LinkConfig::usb`]) and whatever arrives is fed to the link byte
//! for byte; bytes outside frames (a raw monitor, stray text) the link sets
//! aside as text. The chip crate wraps its TX half in the IN-endpoint gate
//! ([`crate::serial::in_endpoint`]) before it hands it here, so esp-println's
//! raw boot and panic text can share the endpoint without a frame's packet
//! being written over.
//!
//! The shape is the comms lab's (`fw-esp32c6/src/tests/comms_lab/usb_pipe.rs`,
//! proven on silicon and the emulated C6):
//!
//! 1. take what the RX FIFO already holds;
//! 2. move log records onto the log channel;
//! 3. write up to [`FRAMES_PER_PASS`] frames, each bounded by
//!    [`WRITE_TIMEOUT`], **reading RX between frame writes** — a frame is
//!    only overdue if its ACK has not *arrived*, not if the ACK waits unread
//!    in the FIFO while this task writes (that was every spurious resend on
//!    the emulator's clean link);
//! 4. sleep until the link's next timer, input, or the transport's doorbell.
//!
//! A write the host does not drain in time is abandoned: the link resends
//! what matters. There is no "host not draining" latch any more (D8): with
//! no host every SYN simply times out, and a stalled host's replies wait in
//! the link's send budget.

use core::sync::atomic::{AtomicBool, Ordering::Relaxed};

use embassy_futures::select::{Either3, select3};
use embassy_time::{Duration, Instant, Timer, with_timeout};
use embedded_io_async::{Read, Write};
use lp_link::{LinkState, Micros};

use super::usb_link_counters;
use super::usb_link_shared::UsbLinkShared;

/// Longest a frame's write may wait for the host to drain the endpoint.
pub const WRITE_TIMEOUT: Duration = Duration::from_millis(250);
/// Frames written per pass before the task looks at the RX side again.
pub const FRAMES_PER_PASS: usize = 8;
/// The longest the task sleeps with nothing to do (the log ring's cadence).
pub const IDLE_CAP_US: Micros = 10_000;
/// Log records moved onto the log channel per pass. Each is popped under its
/// own short critical section (see [`crate::log_ring_logger::pump`]).
const LOG_RECORDS_PER_PASS: usize = 4;
/// Shortest spacing of two SOF samples (see
/// [`crate::serial::usb_connection::DISCONNECT_THRESHOLD`]).
const SOF_SAMPLE_US: Micros = 2_000;
/// The largest frame the link writes: a 256-byte payload with its header and
/// CRC, COBS-FF-encoded and delimited, fits with room to spare.
const FRAME_BYTES: usize = 512;

/// The chip facts the loop needs, supplied by the chip crate (no esp-hal in
/// this crate — ADR 2026-07-29-per-chip-fw-toolchains).
pub trait UsbLinkChip {
    /// Prove liveness to the chip's watchdog feeder (it withholds its feed
    /// while the I/O task is silent).
    fn note_io_alive(&mut self);
    /// Sample the cable: whether a USB host enumerates the board right now
    /// (SOF arriving). Called at most every 2 ms.
    fn host_enumerated(&mut self) -> bool;
    /// The IN endpoint's send buffer is free — read after a write timed out,
    /// to tell a host that stopped draining from a write that never woke.
    fn in_ep_free(&self) -> bool;
    /// Reset the chip (a requested reboot, [`request_reset_when_drained`]).
    fn reset(&mut self) -> !;
}

/// Longest a requested reset waits for the host to acknowledge what the link
/// holds.
const RESET_DRAIN_LIMIT_US: Micros = 1_000_000;

static RESET_WHEN_DRAINED: AtomicBool = AtomicBool::new(false);

/// Reset the chip once the host has everything the link holds — the
/// `Reboot` request's answer above all.
///
/// A reply the server has sent is only *queued* on the link: resetting at
/// once (what the `M!` path did, once the bytes were written) would take
/// the answer down with the board, and the host would see a session reset
/// instead of its reply. So the reboot hook asks, and the link task resets
/// when the link is idle (everything acknowledged), when no host is up to
/// acknowledge anything, or after [`RESET_DRAIN_LIMIT_US`] at most.
pub fn request_reset_when_drained() {
    RESET_WHEN_DRAINED.store(true, Relaxed);
}

/// Run the host link on `rx`/`tx` for ever.
pub async fn run_usb_link<R: Read, W: Write, C: UsbLinkChip>(
    mut rx: R,
    mut tx: W,
    shared: &'static UsbLinkShared,
    mut chip: C,
) -> ! {
    let mut buf = [0u8; 64];
    let mut frame = [0u8; FRAME_BYTES];
    let mut enumerated = true;
    let mut sof_sampled_at: Micros = 0;
    let mut reset_asked_at: Option<Micros> = None;

    loop {
        chip.note_io_alive();
        // SOF comes every 1 ms and its bit is latched: a sample taken sooner
        // than that after the last would count a miss that is not there.
        let now = now_us();
        if now.saturating_sub(sof_sampled_at) >= SOF_SAMPLE_US {
            enumerated = chip.host_enumerated();
            sof_sampled_at = now;
        }
        drain_rx(&mut rx, shared, &mut buf).await;

        let now = now_us();
        shared.with_link(|link| {
            crate::log_ring_logger::pump(link, now, LOG_RECORDS_PER_PASS);
            usb_link_counters::note_stalled(link.is_stalled(now));
        });

        let mut more = false;
        let mut written = 0;
        loop {
            let next = shared.with_link(|link| {
                link.poll_transmit(now_us()).map(|f| {
                    let n = f.len().min(frame.len());
                    frame[..n].copy_from_slice(&f[..n]);
                    (n, f.len())
                })
            });
            let Some((n, len)) = next else { break };
            written += 1;
            if n != len || !enumerated {
                // No host on the cable (or, impossibly, a frame larger than
                // the link's own maximum): not written. The link resends what
                // matters and gives up on a silent peer by itself.
                usb_link_counters::note_frame_discarded_no_host();
            } else {
                match with_timeout(WRITE_TIMEOUT, tx.write_all(&frame[..n])).await {
                    Ok(Ok(())) => {}
                    Ok(Err(_)) => {
                        usb_link_counters::note_write_error();
                        break;
                    }
                    Err(_) => {
                        note_write_timeout(shared, &chip);
                        break;
                    }
                }
                chip.note_io_alive();
            }
            if written >= FRAMES_PER_PASS {
                more = true;
                break;
            }
            drain_rx(&mut rx, shared, &mut buf).await;
        }

        shared.with_link(|link| usb_link_counters::publish(link.counters()));

        if RESET_WHEN_DRAINED.load(Relaxed) {
            let now = now_us();
            let asked = *reset_asked_at.get_or_insert(now);
            let drained =
                shared.with_link(|link| link.state() != LinkState::Established || link.is_idle());
            if drained || now.saturating_sub(asked) >= RESET_DRAIN_LIMIT_US {
                chip.reset();
            }
        }

        let wake = if more {
            now_us()
        } else {
            wake_at(shared, IDLE_CAP_US)
        };
        match select3(
            rx.read(&mut buf),
            Timer::at(Instant::from_micros(wake)),
            shared.doorbell(),
        )
        .await
        {
            Either3::First(Ok(n)) if n > 0 => {
                let t = now_us();
                shared.with_link(|link| link.on_bytes(t, &buf[..n]));
            }
            _ => {}
        }
    }
}

/// The device clock, in the link's unit.
pub fn now_us() -> Micros {
    Instant::now().as_micros()
}

/// Feed the link whatever the RX FIFO already holds, without waiting.
async fn drain_rx<R: Read>(rx: &mut R, shared: &UsbLinkShared, buf: &mut [u8; 64]) {
    loop {
        match embassy_futures::select::select(rx.read(buf), core::future::ready(())).await {
            embassy_futures::select::Either::First(Ok(n)) if n > 0 => {
                let t = now_us();
                shared.with_link(|link| link.on_bytes(t, &buf[..n]));
            }
            _ => return,
        }
    }
}

/// When the link next needs a pass for a timer, capped so the log ring is
/// pumped at least every `cap_us`.
fn wake_at(shared: &UsbLinkShared, cap_us: Micros) -> Micros {
    let now = now_us();
    shared.with_link(|link| {
        link.poll_timeout()
            .unwrap_or(Micros::MAX)
            .min(now + cap_us)
            .max(now)
    })
}

/// A frame write timed out. Only a timeout while a host was draining is
/// news: with no host every SYN times out, and logging those would flush the
/// ring of everything else.
fn note_write_timeout<C: UsbLinkChip>(shared: &UsbLinkShared, chip: &C) {
    let now = now_us();
    let live =
        shared.with_link(|link| link.state() == LinkState::Established && !link.is_stalled(now));
    let in_ep_free = chip.in_ep_free();
    usb_link_counters::note_write_timeout(live, in_ep_free);
    if live {
        log::warn!(
            "[usb_link] a frame write timed out with a host draining ({} so far) at uptime {} ms; \
             in_ep_free={in_ep_free}",
            usb_link_counters::edge().write_timeouts_live,
            Instant::now().as_millis(),
        );
    }
}
