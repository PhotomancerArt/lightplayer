//! Chunked, timeout-bounded writes to a host link's TX half.
//!
//! Every ESP32 firmware writes to its host link the same way: split the buffer
//! into chunks, bound each chunk with a timeout, and do something small between
//! chunks. Only that last part is a chip fact, so it arrives as a hook rather
//! than a dependency — see [`ChunkedWriter::new`].

use core::sync::atomic::{AtomicBool, Ordering::Relaxed};
use embassy_time::{Duration, Instant};
use embedded_hal_async::delay::DelayNs;
use embedded_io_async::Write;

/// The board's resync marker, `00 00 'R' 01 00` (`lpc_wire::RESYNC_SEQUENCE`,
/// which carries the proof that a host's frame scanner reads text again after
/// it from any state; a test below pins the two equal).
///
/// Written before the next bytes after any write that failed or timed out:
/// such a write may have left half a packed frame in the link, with no closing
/// `00`, and a reader stuck inside it would swallow everything after it —
/// the JSON a board falls back to has no `00` to end it. Found at the learned
/// wire dictionary's hardware sitting (a page closed mid-reply; the next
/// connection read "Nothing from this board yet" for minutes).
pub const RESYNC_SEQUENCE: [u8; 5] = [0, 0, b'R', 1, 0];

/// Whether a write failed since the last resync marker went out whole. One
/// flag for the link, not per writer: [`ChunkedWriter`]s are made per write.
/// A bare relaxed atomic, like `link_counters`, so any executor may touch it.
static RESYNC_OWED: AtomicBool = AtomicBool::new(false);

/// How a link chunks and bounds its writes.
///
/// A chip fact, passed in rather than hard-coded, because the two numbers mean
/// different things per transport: USB-Serial-JTAG sizes its chunk for syscall
/// overhead, a UART sizes it for *line time* (the RX FIFO overflows if the TX
/// loop does not yield often enough at 115200 baud).
#[derive(Debug, Clone, Copy)]
pub struct WritePolicy {
    /// Per-chunk timeout. A chunk that does not complete inside it fails the
    /// whole write.
    pub timeout: Duration,
    /// Bytes per chunk.
    pub chunk_size: usize,
    /// What to call the link in error text (`"USB"`, `"UART"`). Surfaces to the
    /// host inside `TransportError::Other`.
    pub link_name: &'static str,
    /// How many times [`ChunkedWriter::write_server_msg`] rewrites a server
    /// frame whose write failed, beyond the first attempt. A chip fact like
    /// the rest of the policy: on a UART a failed write means a transient
    /// stall (a wedged peripheral, or the io task masked through a flash
    /// window) and a rewrite is cheap and honest; on USB-Serial-JTAG a failed
    /// write means the host stopped draining, and rewriting at a dead FIFO
    /// only stalls the io loop the connection monitor is about to latch.
    pub server_msg_retries: usize,
}

impl WritePolicy {
    /// The USB-Serial-JTAG policy used by `fw-esp32c6` and `fw-esp32s3`.
    ///
    /// Timeout: if a chunk doesn't complete in this time, the host is not
    /// draining. A healthy USB full-speed host drains a chunk in well under a
    /// millisecond, so this is still very generous — but short enough that the
    /// frame loop's inline sends stall briefly, not for seconds, in the window
    /// before the not-draining latch kicks in.
    ///
    /// Chunk size: small enough to avoid timeout on slow USB, large enough to
    /// avoid excessive syscalls. Resource snapshots can be 10KB+.
    pub const USB_SERIAL_JTAG: Self = Self {
        timeout: Duration::from_millis(250),
        chunk_size: 256,
        link_name: "USB",
        server_msg_retries: 0,
    };

    /// The UART0 policy used by `fw-esp32v3` (921600 baud through the CH340K).
    ///
    /// Chunk size is *line time*, not syscall overhead: the invariant is
    /// "drain RX at least twice per RX-FIFO fill time". At 921600 baud UART0's
    /// 128-byte RX FIFO holds ~1.4 ms of incoming line, and a 64-byte chunk
    /// costs ~0.7 ms to clock out — the same 2x overflow margin the original
    /// 115200 sizing had, it just drains 8x as often in wall time. The chunk
    /// deliberately does NOT grow with the baud: line time is what protects
    /// the FIFO, not bytes.
    ///
    /// Timeout: a UART TX FIFO drains at line rate whether or not anything
    /// listens, so unlike the USB policy this is not a host-liveness signal —
    /// it is a backstop against a wedged peripheral. It also fires when the
    /// io task goes unpolled for the whole period (executor starvation; see
    /// fw-esp32v3's `serial::io_task` docs and
    /// `docs/debt/shared-uart-io-task-starvation.md`), which is what
    /// [`WriteFailure`]'s chunk/elapsed numbers exist to distinguish.
    pub const UART_921600: Self = Self {
        timeout: Duration::from_millis(250),
        chunk_size: 64,
        link_name: "UART",
        server_msg_retries: 2,
    };
}

/// Capacity for [`IoErrorText`]: generous for a transport error's one-line
/// `Debug` (`Overrun`, `FramingError`, …), small enough to sit on the stack
/// through an interrupt-context write.
const IO_ERROR_TEXT_CAPACITY: usize = 48;

/// A write error's `Debug` text, captured on the stack rather than the heap.
///
/// [`ChunkedWriter::write_chunks`] runs on an interrupt executor on some
/// chips (the classic's swi2 io task), where a heap allocation is forbidden —
/// this used to be built with `alloc::format!` on every write error, which
/// allocated on exactly that executor. See the module docs on
/// [`ChunkedWriter::new`]'s `delay` seam and
/// `docs/adr/2026-08-25-classic-uart-io-task-executor-isolation.md`. This
/// type holds the same one-line detail in a `Copy` fixed-capacity buffer
/// instead; a `Debug` longer than [`IO_ERROR_TEXT_CAPACITY`] is truncated,
/// never allocated or panicked on.
#[derive(Debug, Clone, Copy)]
pub struct IoErrorText {
    bytes: [u8; IO_ERROR_TEXT_CAPACITY],
    len: usize,
}

impl IoErrorText {
    /// Render `error`'s `Debug` form into a fixed buffer. `core::fmt`'s
    /// machinery writes straight into [`IoErrorText`]'s own
    /// [`core::fmt::Write`] impl below, so nothing is heap-allocated along
    /// the way.
    fn from_debug(error: &dyn core::fmt::Debug) -> Self {
        let mut this = Self {
            bytes: [0; IO_ERROR_TEXT_CAPACITY],
            len: 0,
        };
        let _ = core::fmt::Write::write_fmt(&mut this, format_args!("{error:?}"));
        this
    }

    /// The captured text. `core::fmt` only ever writes valid UTF-8, except
    /// when this buffer's capacity cuts a write off mid-codepoint; backing
    /// off to the nearest earlier boundary handles that without panicking.
    pub fn as_str(&self) -> &str {
        let mut end = self.len;
        while end > 0 && core::str::from_utf8(&self.bytes[..end]).is_err() {
            end -= 1;
        }
        // SAFETY: `end` was walked back to the nearest valid UTF-8 boundary
        // not past `self.len`, which the loop above checks directly.
        unsafe { core::str::from_utf8_unchecked(&self.bytes[..end]) }
    }
}

impl core::fmt::Write for IoErrorText {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let remaining = IO_ERROR_TEXT_CAPACITY - self.len;
        let take = remaining.min(s.len());
        self.bytes[self.len..self.len + take].copy_from_slice(&s.as_bytes()[..take]);
        self.len += take;
        Ok(())
    }
}

impl core::fmt::Display for IoErrorText {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where and why a chunked write stopped, for callers that want more than
/// [`ChunkedWriter::write_all`]'s bare bool.
///
/// The chunk/elapsed pair is the diagnostic: a wedged peripheral crawls
/// through every chunk (elapsed ≈ chunk_index × timeout), while a task that
/// simply went unpolled sails through its chunks and then stalls once
/// (elapsed ≈ one timeout, at whatever chunk it happened to be).
#[derive(Debug)]
pub struct WriteFailure {
    /// Zero-based index of the chunk that failed.
    pub chunk_index: usize,
    /// Chunks the whole write would have taken.
    pub chunks_total: usize,
    /// Bytes fully written before the failing chunk.
    pub offset: usize,
    /// Bytes the caller asked to write.
    pub total_bytes: usize,
    /// Time since the write began.
    pub elapsed: Duration,
    /// Debug form of the transport error, or `None` for a timeout.
    pub io_error: Option<IoErrorText>,
}

impl core::fmt::Display for WriteFailure {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match &self.io_error {
            None => write!(
                f,
                "timed out at chunk {}/{} ({} of {} B) after {} ms",
                self.chunk_index + 1,
                self.chunks_total,
                self.offset,
                self.total_bytes,
                self.elapsed.as_millis()
            ),
            Some(error) => write!(
                f,
                "failed at chunk {}/{} ({} of {} B) after {} ms: {error}",
                self.chunk_index + 1,
                self.chunks_total,
                self.offset,
                self.total_bytes,
                self.elapsed.as_millis()
            ),
        }
    }
}

/// A link's TX half plus the policy and per-chunk hook that make writing to it
/// chip-correct.
///
/// Construct one per write; it borrows the transmitter and holds no state of
/// its own, so the cost is nothing and the borrow stays as short as the write.
pub struct ChunkedWriter<'a, W, F, D> {
    pub(crate) tx: &'a mut W,
    pub(crate) policy: WritePolicy,
    on_chunk: F,
    pub(crate) delay: D,
}

impl<'a, W: Write, F: FnMut(), D: DelayNs> ChunkedWriter<'a, W, F, D> {
    /// Wrap `tx` with a write policy, a per-chunk hook, and a delay source.
    ///
    /// `on_chunk` runs **before every chunk**, including the first, and is one
    /// of two crate-specific seams in this module. It exists because a bounded
    /// in-flight write is healthy, not silence, and each firmware has its own
    /// thing to do about that:
    ///
    /// * `fw-esp32c6` / `fw-esp32s3` tick `recovery::watchdog::note_io_alive`,
    ///   so a slow host cannot starve the watchdog feeder into resetting the
    ///   device. The watchdog is a chip fact (it is an `esp-hal` `Rwdt`), so
    ///   this crate may not reach for it — hence the hook.
    /// * `fw-esp32v3` drains its UART RX FIFO, which holds only ~1.4 ms of
    ///   incoming line and would overflow during a multi-second write.
    ///
    /// A `FnMut` rather than a `fn()` precisely so the second kind — which
    /// needs the RX half and the line buffer — fits without a second writer.
    ///
    /// `delay` is the second seam: the source of the per-chunk timeout and the
    /// retry backoff. On the C6/S3 it is `embassy_time::Delay`. On the v3 it
    /// must NOT be — that io task runs on an esp-rtos interrupt executor, and
    /// esp-rtos 0.3.0 never delivers embassy-time wakes to tasks on interrupt
    /// executors (a task that awaits `Timer::after` there parks forever, and
    /// processing such a queue entry while the engine runs can crash the
    /// chip). The v3 passes a delay backed by its own hardware pacer tick,
    /// whose wakes are plain signal wakes. See
    /// `docs/adr/2026-08-25-classic-uart-io-task-executor-isolation.md`.
    pub fn new(tx: &'a mut W, policy: WritePolicy, on_chunk: F, delay: D) -> Self {
        Self {
            tx,
            policy,
            on_chunk,
            delay,
        }
    }

    /// Write all of `data` in chunks with the policy's per-chunk timeout.
    ///
    /// Prevents large messages (e.g. resource snapshots) from timing out
    /// mid-write and corrupting the stream by concatenating with the next
    /// message. Uses `write_all` per chunk to handle partial writes.
    ///
    /// Returns false on the first chunk that times out or errors.
    pub async fn write_all(&mut self, data: &[u8]) -> bool {
        self.try_write_all_with(data, self.policy.timeout)
            .await
            .is_ok()
    }

    /// [`write_all`](Self::write_all) with an explicit per-chunk timeout, for
    /// callers that want a shorter bound than the policy's (the C6/S3
    /// not-draining probe writes a single byte and will not wait long for it).
    pub async fn write_all_with(&mut self, data: &[u8], timeout: Duration) -> bool {
        self.try_write_all_with(data, timeout).await.is_ok()
    }

    /// [`write_all`](Self::write_all), reporting where and why a failed write
    /// stopped instead of a bare false.
    pub async fn try_write_all(&mut self, data: &[u8]) -> Result<(), WriteFailure> {
        self.try_write_all_with(data, self.policy.timeout).await
    }

    /// The one real write loop: every other write method delegates here.
    ///
    /// The per-chunk timeout comes from the `delay` seam, not `embassy_time`
    /// directly — see [`ChunkedWriter::new`] for why that distinction is
    /// load-bearing on the v3.
    ///
    /// After a failed write, the next write first sends [`RESYNC_SEQUENCE`]
    /// (bounded by the same timeout); the debt is paid only once the whole
    /// marker went out, and a marker cut short is simply sent again whole —
    /// it converges from any scanner state, a partial one included.
    pub async fn try_write_all_with(
        &mut self,
        data: &[u8],
        timeout: Duration,
    ) -> Result<(), WriteFailure> {
        if RESYNC_OWED.load(Relaxed) {
            // Still owed if this fails: it is cleared only below.
            self.write_chunks(&RESYNC_SEQUENCE, timeout).await?;
            RESYNC_OWED.store(false, Relaxed);
        }
        self.write_chunks(data, timeout)
            .await
            // Some of these bytes may be on the wire: a reader may now be
            // inside a frame that will never close.
            .inspect_err(|_| RESYNC_OWED.store(true, Relaxed))
    }

    /// [`try_write_all`](Self::try_write_all) for a link that frames and
    /// recovers its own bytes (lp-link, the classic's UART): the same chunks,
    /// timeout and per-chunk hook, but no [`RESYNC_SEQUENCE`] before it and
    /// none owed after a failure. On such a link a cut-short write is a
    /// damaged frame the link resends, and the marker's bytes would only be
    /// one more thing for the host's deframer to discard.
    pub async fn try_write_link_bytes(&mut self, data: &[u8]) -> Result<(), WriteFailure> {
        self.write_chunks(data, self.policy.timeout).await
    }

    async fn write_chunks(&mut self, data: &[u8], timeout: Duration) -> Result<(), WriteFailure> {
        use embassy_futures::select::{Either, select};
        let started = Instant::now();
        let timeout_ms = timeout.as_millis().max(1) as u32;
        let chunks_total = data.len().div_ceil(self.policy.chunk_size).max(1);
        let mut chunk_index = 0;
        let mut offset = 0;
        while offset < data.len() {
            (self.on_chunk)();
            let chunk_end = (offset + self.policy.chunk_size).min(data.len());
            let chunk = &data[offset..chunk_end];
            let io_error =
                match select(self.delay.delay_ms(timeout_ms), self.tx.write_all(chunk)).await {
                    Either::First(_) => None,
                    Either::Second(Err(error)) => Some(IoErrorText::from_debug(&error)),
                    Either::Second(Ok(())) => {
                        offset = chunk_end;
                        chunk_index += 1;
                        continue;
                    }
                };
            return Err(WriteFailure {
                chunk_index,
                chunks_total,
                offset,
                total_bytes: data.len(),
                elapsed: started.elapsed(),
                io_error,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `Write` that fails every call, so the error path below runs with no
    /// real link. `embedded_io_async::ErrorKind` already implements the
    /// `embedded_io` `Error` trait, so no bespoke error type is needed.
    struct FailingWrite;

    impl embedded_io_async::ErrorType for FailingWrite {
        type Error = embedded_io_async::ErrorKind;
    }

    impl embedded_io_async::Write for FailingWrite {
        async fn write(&mut self, _buf: &[u8]) -> Result<usize, Self::Error> {
            Err(embedded_io_async::ErrorKind::Other)
        }
    }

    /// A delay that never completes. [`FailingWrite::write`] above always
    /// resolves on its first poll, so `select` in `write_chunks` never needs
    /// this side to finish — it exists only to satisfy `DelayNs`.
    struct NeverDelay;

    impl embedded_hal_async::delay::DelayNs for NeverDelay {
        async fn delay_ns(&mut self, _ns: u32) {
            core::future::pending::<()>().await
        }
    }

    /// Ticket `2026-09-29-swi2-chunked-writer-allocates-on-error`: a write
    /// error's detail still reaches [`WriteFailure`] once [`format!`] is
    /// gone from the path — this is the behavior the removed `alloc::format!`
    /// call used to provide.
    #[test]
    fn write_error_detail_is_captured_without_format() {
        let mut tx = FailingWrite;
        let mut writer = ChunkedWriter::new(&mut tx, WritePolicy::UART_921600, || {}, NeverDelay);
        let failure = embassy_futures::block_on(writer.try_write_link_bytes(b"hello"))
            .expect_err("FailingWrite always errors");
        let io_error = failure.io_error.expect("io error path");
        let text = io_error.as_str();
        assert!(
            text.contains("Other"),
            "expected the error kind's Debug text, got {text:?}"
        );
    }

    /// Ticket `2026-09-29-swi2-chunked-writer-allocates-on-error`'s "done
    /// when": the error path builds a [`WriteFailure`] with no allocation.
    /// `write_chunks` runs on the classic's swi2 interrupt executor via
    /// [`ChunkedWriter::try_write_link_bytes`]
    /// (`docs/adr/2026-08-25-classic-uart-io-task-executor-isolation.md`),
    /// where a heap allocation is forbidden — so this file's *production*
    /// code (everything before this test module) must never call the
    /// allocating `format!`/`String` machinery. Sliced at the `#[cfg(test)]`
    /// marker so this check of the code above it doesn't trip on its own
    /// source below the marker.
    #[test]
    fn production_code_has_no_heap_allocation_on_the_error_path() {
        let source = include_str!("chunked_write.rs");
        let production = source.split_once("#[cfg(test)]").map_or(source, |(before, _)| before);
        for needle in ["format!(", "alloc::string::String", "String::", "Option<String>"] {
            assert!(
                !production.contains(needle),
                "chunked_write.rs's production code must not contain {needle:?} — \
                 it runs on an interrupt executor with no heap (see this test's docs)"
            );
        }
    }

    /// The board's copy of the marker is the one the host side proves.
    #[cfg(feature = "server")]
    #[test]
    fn the_resync_marker_is_the_wires() {
        assert_eq!(RESYNC_SEQUENCE, lpc_wire::RESYNC_SEQUENCE);
    }
}
