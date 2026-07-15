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

    /// Write all of `bytes` to the device, flushing so the data is actually
    /// sent (serial ports buffer aggressively).
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
            Self::Io(message) => write!(f, "byte stream I/O error: {message}"),
        }
    }
}

impl std::error::Error for ByteStreamError {}
