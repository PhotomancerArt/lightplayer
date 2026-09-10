//! One serial-class device attachment as a raw byte pipe.

use core::fmt;

/// One serial-class device attachment as a raw byte pipe.
///
/// SERIAL connector class only — websocket/server devices (future) attach at
/// a higher level and never see DTR/RTS.
///
/// The trait is deliberately **sync**: the existing hardware transport
/// (`transport_serial::hardware`) drives the port from a dedicated thread
/// with non-blocking reads, so a sync trait driven by that thread is the
/// honest seam. The M3 phase file explicitly allows this shape when the
/// thread-based model makes an async trait awkward — the seam matters, not
/// the flavor.
///
/// Implementations must be [`Send`] so the transport thread can own them.
pub trait DeviceByteStream: Send {
    /// Read whatever bytes are currently available into `buf`.
    ///
    /// Returns `Ok(0)` when no data is available *right now* (the caller
    /// should back off briefly and poll again). A device that is gone for
    /// good returns [`ByteStreamError::Closed`].
    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, ByteStreamError>;

    /// Write all of `bytes` to the device.
    ///
    /// Must return in bounded time — the framing thread that owns the stream
    /// can only honor a shutdown between calls, so an implementation that
    /// blocks forever pins the thread and whatever OS resource it holds
    /// (`docs/defects/2026-09-08-serial-close-leaks-the-port-on-a-wedged-device.md`).
    /// Handing the bytes to the OS is the contract; WAITING for them to
    /// reach the wire is not, and on a serial port that wait (`tcdrain`) is
    /// exactly the unbounded call to avoid.
    ///
    /// An implementation that gives up on a timeout reports
    /// [`ByteStreamError::WriteStalled`], which is a dropped frame rather
    /// than a dead stream.
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), ByteStreamError>;

    /// Drive the DTR/RTS control lines.
    ///
    /// Each pin is optional so callers can reproduce hardware reset dances
    /// pin-write-for-pin-write (the espflash sequences interleave single-pin
    /// writes; forcing both pins per call would inject extra edges that real
    /// ESP32 reset circuits key on). `None` leaves that pin untouched.
    fn set_signals(&mut self, dtr: Option<bool>, rts: Option<bool>) -> Result<(), ByteStreamError>;

    /// Close and reopen the attachment at a (possibly different) baud rate.
    ///
    /// Present for the M6 bootloader protocol (esptool switches baud rates
    /// mid-session); fakes may treat it as a buffer flush.
    fn reopen(&mut self, baud_rate: u32) -> Result<(), ByteStreamError>;
}

/// Error surface for [`DeviceByteStream`] operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ByteStreamError {
    /// The device is gone (unplugged, EOF, or deliberately disconnected).
    Closed,
    /// The device is not taking output: the write did not finish inside the
    /// stream's timeout because the peer stopped draining its receive FIFO.
    ///
    /// Explicitly NOT [`Self::Closed`], and the distinction is the whole
    /// point of the variant. A board that has stopped reading — a C6 spinning
    /// in its bootloader — is the `Unresponsive` case device management
    /// exists to repair; reporting it as a dead stream tears the link down
    /// and classifies a repairable board as `Gone`, which is a state no
    /// management operation runs from.
    ///
    /// The frame that hit this may have been PARTIALLY written. Callers drop
    /// it and carry on rather than retrying: the line protocol resynchronizes
    /// at the next newline, the readiness engine re-asks on its own cadence,
    /// and any repair ends in a fresh link anyway.
    WriteStalled,
    /// Any other I/O failure, with the underlying message.
    Io(String),
}

impl ByteStreamError {
    pub fn io(message: impl Into<String>) -> Self {
        Self::Io(message.into())
    }
}

impl fmt::Display for ByteStreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => f.write_str("byte stream closed"),
            Self::WriteStalled => f.write_str("device is not accepting output"),
            Self::Io(message) => write!(f, "byte stream I/O error: {message}"),
        }
    }
}

impl std::error::Error for ByteStreamError {}
