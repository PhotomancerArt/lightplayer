//! Composable failure-injection knobs for the fake device's byte stream.
//!
//! Knobs live on the STREAM (transport conditions), not on the script
//! (device behavior): a booting LightPlayer with a stalling wire is a
//! different test than a device stuck in the ROM downloader.

use std::time::Duration;

/// Failure plan applied to the device→host read path (and write latency on
/// the host→device path). All knobs compose; byte offsets count cumulative
/// bytes SERVED to the reader since the plan was installed.
#[derive(Clone, Debug, Default)]
pub struct FakeFailurePlan {
    /// Delay between output bytes becoming available and being readable.
    pub read_latency: Duration,
    /// Sleep this long inside every host→device write (slow wire).
    pub write_latency: Duration,
    /// After serving this many bytes, stop responding — reads return
    /// "no data" forever, with no EOF (the mid-frame timeout condition when
    /// combined with [`Self::cut_mid_frame_after_frames`]).
    pub stall_read_after_bytes: Option<usize>,
    /// After serving this many bytes, the device is GONE: reads fail with
    /// `ByteStreamError::Closed` (EOF / unplug).
    pub disconnect_read_after_bytes: Option<usize>,
    /// XOR the byte at this cumulative offset with 0xFF (garbled wire).
    pub garble_byte_at: Option<usize>,
    /// Drop the byte at this cumulative offset entirely.
    pub drop_byte_at: Option<usize>,
    /// Truncate the Nth (0-based) protocol frame halfway, then stall — the
    /// io_task mid-frame condition: a frame that starts but never finishes.
    pub cut_mid_frame_after_frames: Option<usize>,
    /// Interleave this non-`M!` log line before every protocol frame (logs
    /// and frames share the wire on real hardware).
    pub log_flood_line: Option<String>,
}

impl FakeFailurePlan {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn with_read_latency(mut self, latency: Duration) -> Self {
        self.read_latency = latency;
        self
    }

    pub fn with_write_latency(mut self, latency: Duration) -> Self {
        self.write_latency = latency;
        self
    }

    pub fn with_stall_after_bytes(mut self, bytes: usize) -> Self {
        self.stall_read_after_bytes = Some(bytes);
        self
    }

    pub fn with_disconnect_after_bytes(mut self, bytes: usize) -> Self {
        self.disconnect_read_after_bytes = Some(bytes);
        self
    }

    pub fn with_garble_byte_at(mut self, offset: usize) -> Self {
        self.garble_byte_at = Some(offset);
        self
    }

    pub fn with_drop_byte_at(mut self, offset: usize) -> Self {
        self.drop_byte_at = Some(offset);
        self
    }

    pub fn with_cut_mid_frame_after_frames(mut self, frames: usize) -> Self {
        self.cut_mid_frame_after_frames = Some(frames);
        self
    }

    pub fn with_log_flood_line(mut self, line: impl Into<String>) -> Self {
        self.log_flood_line = Some(line.into());
        self
    }
}
