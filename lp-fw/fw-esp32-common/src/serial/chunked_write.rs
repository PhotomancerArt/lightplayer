//! Chunked, timeout-bounded writes to a host link's TX half.
//!
//! Every ESP32 firmware writes to its host link the same way: split the buffer
//! into chunks, bound each chunk with a timeout, and do something small between
//! chunks. Only that last part is a chip fact, so it arrives as a hook rather
//! than a dependency — see [`ChunkedWriter::new`].

use embassy_time::{Duration, Timer};
use embedded_io_async::Write;

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
    };
}

/// A link's TX half plus the policy and per-chunk hook that make writing to it
/// chip-correct.
///
/// Construct one per write; it borrows the transmitter and holds no state of
/// its own, so the cost is nothing and the borrow stays as short as the write.
pub struct ChunkedWriter<'a, W, F> {
    pub(crate) tx: &'a mut W,
    pub(crate) policy: WritePolicy,
    on_chunk: F,
}

impl<'a, W: Write, F: FnMut()> ChunkedWriter<'a, W, F> {
    /// Wrap `tx` with a write policy and a per-chunk hook.
    ///
    /// `on_chunk` runs **before every chunk**, including the first, and is the
    /// single crate-specific seam in this module. It exists because a bounded
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
    pub fn new(tx: &'a mut W, policy: WritePolicy, on_chunk: F) -> Self {
        Self {
            tx,
            policy,
            on_chunk,
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
        self.write_all_with(data, self.policy.timeout).await
    }

    /// [`write_all`](Self::write_all) with an explicit per-chunk timeout, for
    /// callers that want a shorter bound than the policy's (the C6/S3
    /// not-draining probe writes a single byte and will not wait long for it).
    pub async fn write_all_with(&mut self, data: &[u8], timeout: Duration) -> bool {
        use embassy_futures::select::{Either, select};
        let mut offset = 0;
        while offset < data.len() {
            (self.on_chunk)();
            let chunk_end = (offset + self.policy.chunk_size).min(data.len());
            let chunk = &data[offset..chunk_end];
            match select(Timer::after(timeout), self.tx.write_all(chunk)).await {
                Either::First(_) => return false,
                Either::Second(Err(_)) => return false,
                Either::Second(Ok(())) => {}
            }
            offset = chunk_end;
        }
        true
    }
}
