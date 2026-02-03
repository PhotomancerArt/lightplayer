//! Serial host implementation for emulator
//!
//! Handles serial communication between guest and host with buffer management.

extern crate alloc;

use alloc::collections::VecDeque;
use lp_riscv_emu_shared::SERIAL_ERROR_BUFFER_FULL;

use log;

/// Serial host for managing bidirectional serial communication
pub struct HostSerial {
    to_guest_buf: VecDeque<u8>,   // Host → Guest (guest reads from this)
    from_guest_buf: VecDeque<u8>, // Guest → Host (guest writes to this)
}

/// Serial error for host-side operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SerialError {
    /// Buffer is full (128KB limit exceeded)
    BufferFull,
}

impl core::fmt::Display for SerialError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SerialError::BufferFull => write!(f, "Serial buffer full"),
        }
    }
}

impl HostSerial {
    /// Default buffer size for HostSerial (128KB)
    pub const DEFAULT_BUF_SIZE: usize = 128 * 1024;

    /// Create a new SerialHost instance
    ///
    /// # Arguments
    /// * `buffer_size` - Capacity hint for buffers (actual limit is 128KB)
    pub fn new(buffer_size: usize) -> Self {
        HostSerial {
            to_guest_buf: VecDeque::with_capacity(buffer_size),
            from_guest_buf: VecDeque::with_capacity(buffer_size),
        }
    }

    /// Handles guest writing data to host
    /// Called by the handler for SYSCALL_SERIAL_WRITE
    ///
    /// # Arguments
    /// * `buffer` - Bytes to write
    ///
    /// # Returns
    /// * Positive number: bytes written
    /// * Negative number: error code (SERIAL_ERROR_BUFFER_FULL)
    pub fn guest_write(&mut self, buffer: &[u8]) -> i32 {
        const MAX_BUFFER_SIZE: usize = 128 * 1024;

        // Calculate available space
        let available = MAX_BUFFER_SIZE.saturating_sub(self.from_guest_buf.len());
        let to_write = buffer.len().min(available);

        if to_write == 0 && buffer.len() > 0 {
            // Buffer full
            return SERIAL_ERROR_BUFFER_FULL;
        }

        // Write bytes (drop excess if buffer would exceed limit)
        if to_write > 0 {
            self.from_guest_buf.extend(&buffer[..to_write]);
            log::trace!(
                "HostSerial::guest_write: Wrote {} bytes, from_guest_buf now has {} bytes",
                to_write,
                self.from_guest_buf.len()
            );
        }

        to_write as i32
    }

    /// Handles guest reading data from host
    /// Called by the handler for SYSCALL_SERIAL_READ
    ///
    /// # Arguments
    /// * `buffer` - Buffer to read into
    ///
    /// # Returns
    /// * Positive number: bytes read
    /// * Zero: no data available
    pub fn guest_read(&mut self, buffer: &mut [u8]) -> i32 {
        if buffer.is_empty() {
            return 0;
        }

        if self.to_guest_buf.is_empty() {
            log::trace!("HostSerial::guest_read: Buffer empty, returning 0");
            return 0;
        }

        // Read available bytes (up to buffer length)
        let to_read = buffer.len().min(self.to_guest_buf.len());
        log::trace!(
            "HostSerial::guest_read: Reading {} bytes from buffer (buffer has {} bytes)",
            to_read,
            self.to_guest_buf.len()
        );
        for i in 0..to_read {
            if let Some(byte) = self.to_guest_buf.pop_front() {
                buffer[i] = byte;
            } else {
                log::warn!(
                    "HostSerial::guest_read: Unexpected empty buffer at index {}",
                    i
                );
                return i as i32;
            }
        }

        // Log first 50 bytes of what we're reading
        let preview_len = to_read.min(50);
        let hex_preview: alloc::vec::Vec<alloc::string::String> = buffer[..preview_len]
            .iter()
            .map(|b| alloc::format!("{:02x}", b))
            .collect();
        log::trace!(
            "HostSerial::guest_read: Read {} bytes (first 50 hex): {}",
            to_read,
            hex_preview.join(" ")
        );

        to_read as i32
    }

    /// Check if guest has data available to read
    /// Called by the handler for SYSCALL_SERIAL_HAS_DATA
    ///
    /// # Returns
    /// * `true` if data is available
    /// * `false` otherwise
    pub fn has_data(&self) -> bool {
        !self.to_guest_buf.is_empty()
    }

    /// Handles host reading data from guest
    /// Called by the user of the emulator to read data from the guest
    ///
    /// # Arguments
    /// * `buffer` - Buffer to read into
    ///
    /// # Returns
    /// * `Ok(usize)` - Bytes read
    pub fn host_read(&mut self, buffer: &mut [u8]) -> Result<usize, SerialError> {
        if buffer.is_empty() {
            return Ok(0);
        }

        if self.from_guest_buf.is_empty() {
            log::trace!("HostSerial::host_read: from_guest_buf is empty, returning 0");
            return Ok(0);
        }

        // Read available bytes (up to buffer length)
        let to_read = buffer.len().min(self.from_guest_buf.len());
        log::trace!(
            "HostSerial::host_read: Reading {} bytes from from_guest_buf (buffer has {} bytes)",
            to_read,
            self.from_guest_buf.len()
        );
        for i in 0..to_read {
            if let Some(byte) = self.from_guest_buf.pop_front() {
                buffer[i] = byte;
            } else {
                return Ok(i);
            }
        }

        log::trace!(
            "HostSerial::host_read: Read {} bytes, from_guest_buf now has {} bytes",
            to_read,
            self.from_guest_buf.len()
        );

        Ok(to_read)
    }

    /// Handles host writing data to guest
    /// Called by the user of the emulator to send data to the guest
    ///
    /// # Arguments
    /// * `buffer` - Bytes to write
    ///
    /// # Returns
    /// * `Ok(usize)` - Bytes written
    /// * `Err(SerialError::BufferFull)` - Buffer is full
    pub fn host_write(&mut self, buffer: &[u8]) -> Result<usize, SerialError> {
        const MAX_BUFFER_SIZE: usize = 128 * 1024;

        // Calculate available space
        let available = MAX_BUFFER_SIZE.saturating_sub(self.to_guest_buf.len());
        let to_write = buffer.len().min(available);

        if to_write == 0 && buffer.len() > 0 {
            return Err(SerialError::BufferFull);
        }

        // Write bytes (drop excess if buffer would exceed limit)
        if to_write > 0 {
            // Log first 50 bytes of what we're writing
            let preview_len = to_write.min(50);
            let hex_preview: alloc::vec::Vec<alloc::string::String> = buffer[..preview_len]
                .iter()
                .map(|b| alloc::format!("{:02x}", b))
                .collect();
            log::trace!(
                "HostSerial::host_write: Writing {} bytes (first 50 hex): {}",
                to_write,
                hex_preview.join(" ")
            );
            self.to_guest_buf.extend(&buffer[..to_write]);
            log::trace!(
                "HostSerial::host_write: to_guest_buf now has {} bytes",
                self.to_guest_buf.len()
            );
        }

        Ok(to_write)
    }

    /// Read a line from guest (until newline or EOF)
    /// Reads from the buffer that guest writes to
    ///
    /// # Returns
    /// * `Ok(String)` - Line read (without newline)
    /// * `Ok(String)` - Partial line if no newline found and buffer is empty
    pub fn host_read_line(&mut self) -> alloc::string::String {
        let mut line = alloc::string::String::new();

        // Find newline in buffer
        if let Some(newline_pos) = self
            .from_guest_buf
            .iter()
            .position(|&b| b == b'\n' || b == b'\r')
        {
            // Read up to and including newline
            for _ in 0..=newline_pos {
                if let Some(byte) = self.from_guest_buf.pop_front() {
                    if byte != b'\n' && byte != b'\r' {
                        line.push(byte as char);
                    }
                }
            }
            return line;
        }

        // No newline found, return what we have
        while let Some(byte) = self.from_guest_buf.pop_front() {
            line.push(byte as char);
        }
        line
    }

    /// Write a line to guest (appends newline)
    /// Writes to the buffer that guest reads from
    ///
    /// # Arguments
    /// * `line` - Line to write (without newline)
    ///
    /// # Returns
    /// * `Ok(usize)` - Bytes written (including newline)
    /// * `Err(SerialError::BufferFull)` - Buffer is full
    pub fn host_write_line(&mut self, line: &str) -> Result<usize, SerialError> {
        const MAX_BUFFER_SIZE: usize = 128 * 1024;

        // Calculate total bytes needed (line + newline)
        let total_bytes = line.len() + 1;

        // Calculate available space
        let available = MAX_BUFFER_SIZE.saturating_sub(self.to_guest_buf.len());

        if total_bytes > available {
            return Err(SerialError::BufferFull);
        }

        // Write line bytes
        self.to_guest_buf.extend(line.as_bytes());
        // Write newline
        self.to_guest_buf.push_back(b'\n');

        Ok(total_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_guest_write_basic() {
        let mut serial = HostSerial::new(HostSerial::DEFAULT_BUF_SIZE);
        let data = b"hello";
        let result = serial.guest_write(data);
        assert_eq!(result, 5);
        assert_eq!(serial.from_guest_buf.len(), 5);
    }

    #[test]
    fn test_guest_read_basic() {
        let mut serial = HostSerial::new(HostSerial::DEFAULT_BUF_SIZE);
        serial.to_guest_buf.extend(b"hello");
        let mut buf = [0u8; 10];
        let result = serial.guest_read(&mut buf);
        assert_eq!(result, 5);
        assert_eq!(&buf[..5], b"hello");
        assert!(serial.to_guest_buf.is_empty());
    }

    #[test]
    fn test_buffer_size_limit() {
        let mut serial = HostSerial::new(HostSerial::DEFAULT_BUF_SIZE);
        // Fill buffer to limit
        let large_data = vec![0u8; HostSerial::DEFAULT_BUF_SIZE];
        let result = serial.guest_write(&large_data);
        assert_eq!(result, HostSerial::DEFAULT_BUF_SIZE as i32);

        // Try to write more - should return error
        let result = serial.guest_write(b"extra");
        assert_eq!(result, SERIAL_ERROR_BUFFER_FULL);
    }

    #[test]
    fn test_fifo_behavior() {
        let mut serial = HostSerial::new(HostSerial::DEFAULT_BUF_SIZE);
        serial.to_guest_buf.extend(b"hello");
        serial.to_guest_buf.extend(b"world");

        let mut buf = [0u8; 5];
        let result = serial.guest_read(&mut buf);
        assert_eq!(result, 5);
        assert_eq!(&buf, b"hello");

        let result = serial.guest_read(&mut buf);
        assert_eq!(result, 5);
        assert_eq!(&buf, b"world");
    }

    #[test]
    fn test_has_data() {
        let mut serial = HostSerial::new(HostSerial::DEFAULT_BUF_SIZE);
        assert!(!serial.has_data());
        serial.to_guest_buf.extend(b"test");
        assert!(serial.has_data());
    }

    #[test]
    fn test_partial_read() {
        let mut serial = HostSerial::new(HostSerial::DEFAULT_BUF_SIZE);
        serial.to_guest_buf.extend(b"hello world");
        let mut buf = [0u8; 5];
        let result = serial.guest_read(&mut buf);
        assert_eq!(result, 5);
        assert_eq!(&buf, b"hello");
        assert_eq!(serial.to_guest_buf.len(), 6); // " world" remaining
    }

    #[test]
    fn test_host_write_to_guest_read() {
        let mut serial = HostSerial::new(HostSerial::DEFAULT_BUF_SIZE);
        let data = b"test data";
        let result = serial.host_write(data);
        assert_eq!(result, Ok(9));

        // Guest reads from to_guest_buf
        let mut buf = [0u8; 20];
        let result = serial.guest_read(&mut buf);
        assert_eq!(result, 9);
        assert_eq!(&buf[..9], data);
    }

    #[test]
    fn test_guest_write_to_host_read() {
        let mut serial = HostSerial::new(HostSerial::DEFAULT_BUF_SIZE);
        let data = b"guest data";
        let result = serial.guest_write(data);
        assert_eq!(result, 10); // "guest data" is 10 bytes

        // Host reads from from_guest_buf
        let mut buf = [0u8; 20];
        let result = serial.host_read(&mut buf);
        assert_eq!(result, Ok(10));
        assert_eq!(&buf[..10], data);
    }

    #[test]
    fn test_empty_read() {
        let mut serial = HostSerial::new(HostSerial::DEFAULT_BUF_SIZE);
        let mut buf = [0u8; 10];
        let result = serial.guest_read(&mut buf);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_buffer_full_error() {
        let mut serial = HostSerial::new(HostSerial::DEFAULT_BUF_SIZE);
        let large_data = vec![0u8; HostSerial::DEFAULT_BUF_SIZE];
        let result = serial.host_write(&large_data);
        assert_eq!(result, Ok(HostSerial::DEFAULT_BUF_SIZE));

        let result = serial.host_write(b"extra");
        assert_eq!(result, Err(SerialError::BufferFull));
    }

    #[test]
    fn test_read_smaller_than_available() {
        let mut serial = HostSerial::new(HostSerial::DEFAULT_BUF_SIZE);
        serial.to_guest_buf.extend(b"hello");
        let mut buf = [0u8; 3];
        let result = serial.guest_read(&mut buf);
        assert_eq!(result, 3);
        assert_eq!(&buf, b"hel");
        assert_eq!(serial.to_guest_buf.len(), 2); // "lo" remaining
    }

    #[test]
    fn test_read_larger_than_available() {
        let mut serial = HostSerial::new(HostSerial::DEFAULT_BUF_SIZE);
        serial.to_guest_buf.extend(b"hi");
        let mut buf = [0u8; 10];
        let result = serial.guest_read(&mut buf);
        assert_eq!(result, 2);
        assert_eq!(&buf[..2], b"hi");
        assert!(serial.to_guest_buf.is_empty());
    }

    #[test]
    fn test_host_read_line() {
        let mut serial = HostSerial::new(HostSerial::DEFAULT_BUF_SIZE);
        serial.from_guest_buf.extend(b"hello\nworld\n");

        let line1 = serial.host_read_line();
        assert_eq!(line1, "hello");

        let line2 = serial.host_read_line();
        assert_eq!(line2, "world");
    }

    #[test]
    fn test_host_read_line_partial() {
        let mut serial = HostSerial::new(HostSerial::DEFAULT_BUF_SIZE);
        serial.from_guest_buf.extend(b"partial");

        let line = serial.host_read_line();
        assert_eq!(line, "partial");
        assert!(serial.from_guest_buf.is_empty());
    }

    #[test]
    fn test_host_read_line_empty() {
        let mut serial = HostSerial::new(HostSerial::DEFAULT_BUF_SIZE);

        let line = serial.host_read_line();
        assert_eq!(line, "");
    }

    #[test]
    fn test_host_write_line() {
        let mut serial = HostSerial::new(HostSerial::DEFAULT_BUF_SIZE);

        let result = serial.host_write_line("hello");
        assert_eq!(result, Ok(6)); // "hello\n" is 6 bytes

        let mut buf = [0u8; 10];
        let read_result = serial.guest_read(&mut buf);
        assert_eq!(read_result, 6);
        assert_eq!(&buf[..6], b"hello\n");
    }

    #[test]
    fn test_host_write_line_multiple() {
        let mut serial = HostSerial::new(HostSerial::DEFAULT_BUF_SIZE);

        let _ = serial.host_write_line("line1");
        let _ = serial.host_write_line("line2");

        let mut buf = [0u8; 20];
        let read_result = serial.guest_read(&mut buf);
        assert_eq!(read_result, 12); // "line1\nline2\n" is 12 bytes
        assert_eq!(&buf[..12], b"line1\nline2\n");
    }

    #[test]
    fn test_host_write_line_buffer_full() {
        let mut serial = HostSerial::new(HostSerial::DEFAULT_BUF_SIZE);
        // Fill buffer to near limit
        let large_data = alloc::vec![0u8; HostSerial::DEFAULT_BUF_SIZE - 10];
        let _ = serial.host_write(&large_data);

        // Try to write a line that would exceed limit
        let result = serial.host_write_line("this is too long");
        assert_eq!(result, Err(SerialError::BufferFull));
    }
}

#[cfg(test)]
mod integration_tests {
    use crate::serial::test_serial::serial_pair;

    #[test]
    fn test_guest_serial_with_serial_host() {
        let (host, guest) = serial_pair();

        // Host writes data
        let _ = host.write(b"hello\nworld\n");

        // Guest reads lines
        let line1 = guest.read_line();
        assert_eq!(line1, "hello");

        let line2 = guest.read_line();
        assert_eq!(line2, "world");

        // Guest writes data
        let write_result = guest.write(b"response\n");
        assert!(write_result > 0);

        // Host reads data
        let mut buf = [0u8; 20];
        let result = host.read(&mut buf);
        assert_eq!(result, Ok(9));
        assert_eq!(&buf[..9], b"response\n");
    }

    #[test]
    fn test_guest_serial_read_line_with_data() {
        let (host, guest) = serial_pair();

        // Host writes data
        let _ = host.write(b"hello\nworld\n");

        // Guest reads lines
        let line1 = guest.read_line();
        assert_eq!(line1, "hello");

        let line2 = guest.read_line();
        assert_eq!(line2, "world");
    }

    #[test]
    fn test_guest_serial_partial_line() {
        let (host, guest) = serial_pair();

        // Host writes partial line
        let _ = host.write(b"partial");

        // Read should return the partial data (no newline, but data exists)
        let line = guest.read_line();
        assert_eq!(line, "partial");

        // Add newline and read again
        let _ = host.write(b" line\n");

        let line = guest.read_line();
        assert_eq!(line, " line");
    }

    #[test]
    fn test_guest_serial_write_and_host_read() {
        let (host, guest) = serial_pair();

        // Guest writes data
        let result = guest.write(b"test message\n");
        assert!(result > 0);

        // Host reads data
        let mut buf = [0u8; 20];
        let result = host.read(&mut buf);
        assert_eq!(result, Ok(13));
        assert_eq!(&buf[..13], b"test message\n");
    }

    #[test]
    fn test_host_write_line_and_guest_read_line() {
        let (host, guest) = serial_pair();

        // Host writes lines
        let _ = host.write_line("hello");
        let _ = host.write_line("world");

        // Guest reads lines
        let line1 = guest.read_line();
        assert_eq!(line1, "hello");

        let line2 = guest.read_line();
        assert_eq!(line2, "world");
    }

    #[test]
    fn test_guest_write_line_and_host_read_line() {
        let (host, guest) = serial_pair();

        // Guest writes lines
        let result1 = guest.write_line("message1");
        assert!(result1 > 0);

        let result2 = guest.write_line("message2");
        assert!(result2 > 0);

        // Host reads lines
        let line1 = host.read_line();
        assert_eq!(line1, "message1");

        let line2 = host.read_line();
        assert_eq!(line2, "message2");
    }

    #[test]
    fn test_bidirectional_line_communication() {
        let (host, guest) = serial_pair();

        // Host sends command
        let _ = host.write_line("echo test");

        // Guest reads command and responds
        let command = guest.read_line();
        assert_eq!(command, "echo test");

        let _ = guest.write_line("test");

        // Host reads response
        let response = host.read_line();
        assert_eq!(response, "test");
    }
}
