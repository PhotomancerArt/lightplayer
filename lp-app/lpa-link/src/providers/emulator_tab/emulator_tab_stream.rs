//! [`DeviceByteStream`] over an emulated board hosted in this tab.
//!
//! The whole of mode A's link is this file plus
//! [`ByteStreamLink`](crate::device_link::byte_stream::ByteStreamLink): a
//! board in a Worker is a byte pipe with DTR/RTS, which is exactly what the
//! seam describes, so there is no second `Link` type and no second pump (D4).
//!
//! # Why the reset dance needs no delays
//!
//! `ByteStreamLink::run_reset` writes the pin sequences with **no**
//! inter-step holds, because it may not block the caller. That is right
//! here for the same reason it is right for the fake device: the emulator
//! decodes the reset from the pin EDGES rather than pattern-matching a
//! timed sequence (`lp-emu/esp/README.md`, "decoded, not pattern-matched"),
//! so the ROM download dance lands whatever the wall-clock gaps were. Real
//! silicon is the case that needs the ~100 ms holds, and it has a thread to
//! sleep on.
//!
//! # Baud is not a thing here
//!
//! The board's link is USB-Serial-JTAG: there is no line rate to set, and
//! the machine ignores one. `reopen` takes the model's baud and drops it
//! rather than pretending a number was applied.

use std::collections::VecDeque;

use lpa_client::stream::{ByteStreamError, DeviceByteStream};

use super::emulator_tab_bridge::EmulatorTabPort;

/// Bytes an emulated board has said, waiting for the model to read them.
pub struct EmulatorTabStream {
    port: EmulatorTabPort,
    /// What one drain handed over and the caller's buffer could not take.
    /// The page's buffer is drained whole (one call, not one byte at a
    /// time); the remainder waits here for the next `read_available`.
    pending: VecDeque<u8>,
}

impl EmulatorTabStream {
    pub fn new(port: EmulatorTabPort) -> Self {
        Self {
            port,
            pending: VecDeque::new(),
        }
    }

    /// The port this stream speaks through, for the control handle that
    /// shares it.
    pub fn port(&self) -> EmulatorTabPort {
        self.port
    }
}

impl DeviceByteStream for EmulatorTabStream {
    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, ByteStreamError> {
        // A failure on the page's work queue is the link's error event: a
        // write that could not be applied, an open that was refused. It is
        // reported BEFORE any bytes so the model hears the failure in the
        // order it happened.
        if let Some(error) = self.port.take_error() {
            return Err(ByteStreamError::io(error));
        }
        if self.pending.is_empty() {
            // The one failure this call has is the handle being gone —
            // `dispose` took it — which is a closed stream, not an IO
            // error the model should try to narrate.
            let bytes = self
                .port
                .take_bytes()
                .map_err(|_| ByteStreamError::Closed)?;
            self.pending.extend(bytes);
        }
        let mut written = 0;
        while written < buf.len() {
            let Some(byte) = self.pending.pop_front() else {
                break;
            };
            buf[written] = byte;
            written += 1;
        }
        Ok(written)
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), ByteStreamError> {
        // Queued, not sent: the page applies it behind whatever is already
        // in flight (the bridge's one chain), so a frame written the
        // instant the port opened still lands after the open.
        self.port
            .write(bytes)
            .map_err(|error| ByteStreamError::io(error.to_string()))
    }

    fn set_signals(&mut self, dtr: Option<bool>, rts: Option<bool>) -> Result<(), ByteStreamError> {
        self.port
            .signals(dtr, rts)
            .map_err(|error| ByteStreamError::io(error.to_string()))
    }

    fn reopen(&mut self, _baud_rate: u32) -> Result<(), ByteStreamError> {
        self.port
            .reopen()
            .map_err(|error| ByteStreamError::io(error.to_string()))
    }
}
