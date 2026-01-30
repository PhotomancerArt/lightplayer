//! Serial I/O trait for firmware communication
//!
//! Provides a simple, synchronous interface for reading and writing raw bytes.
//! The transport layer handles message framing, buffering, and JSON parsing.

extern crate alloc;

use alloc::string::String;

/// Error type for serial I/O operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SerialError {
    /// Write operation failed
    WriteFailed(String),
    /// Read operation failed
    ReadFailed(String),
    /// Other serial error
    Other(String),
}

impl core::fmt::Display for SerialError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SerialError::WriteFailed(msg) => write!(f, "Write failed: {msg}"),
            SerialError::ReadFailed(msg) => write!(f, "Read failed: {msg}"),
            SerialError::Other(msg) => write!(f, "Serial error: {msg}"),
        }
    }
}

/// Trait for serial I/O operations
///
/// Provides a simple, synchronous interface for reading and writing raw bytes.
/// Implementations can use blocking or async I/O internally, but the interface
/// is synchronous to keep the transport layer simple.
pub trait SerialIo {
    /// Write bytes to the serial port (blocking)
    ///
    /// This is a blocking operation that writes all bytes before returning.
    /// For async implementations, this can be a wrapper that blocks on the async write.
    ///
    /// # Arguments
    /// * `data` - Bytes to write
    ///
    /// # Returns
    /// * `Ok(())` if all bytes were written successfully
    /// * `Err(SerialError)` if writing failed
    fn write(&mut self, data: &[u8]) -> Result<(), SerialError>;

    /// Read available bytes from the serial port (non-blocking)
    ///
    /// Reads up to `buf.len()` bytes that are currently available.
    /// Returns immediately with whatever data is available (may be 0 bytes).
    /// Does not block waiting for data.
    ///
    /// # Arguments
    /// * `buf` - Buffer to read into
    ///
    /// # Returns
    /// * `Ok(n)` - Number of bytes read (0 if no data available)
    /// * `Err(SerialError)` if reading failed
    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, SerialError>;

    /// Check if data is available to read (optional optimization)
    ///
    /// Returns `true` if `read_available()` would return at least 1 byte.
    /// This is an optimization hint - implementations can always return `true`
    /// and let `read_available()` return 0 if no data is available.
    ///
    /// # Returns
    /// * `true` if data is available
    /// * `false` if no data is available
    fn has_data(&self) -> bool {
        // Default implementation always returns true
        // Implementations can override for optimization
        true
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use alloc::format;

    #[test]
    fn test_serial_error_display() {
        let err = SerialError::WriteFailed("test".into());
        assert!(format!("{}", err).contains("Write failed"));
    }
}
