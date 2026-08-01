//! Serial error code constants shared between host and guest

use alloc::collections::VecDeque;
use alloc::string::String;

/// Serial error: Invalid pointer (guest provided invalid memory address)
pub const SERIAL_ERROR_INVALID_POINTER: i32 = -1;

/// Serial error: Buffer full (128KB limit exceeded)
pub const SERIAL_ERROR_BUFFER_FULL: i32 = -2;

/// Serial error: Buffer not allocated (lazy allocation not yet done)
pub const SERIAL_ERROR_BUFFER_NOT_ALLOCATED: i32 = -3;

/// Trait for serial syscall operations
/// Allows GuestSerial to work with both actual syscalls (on guest) and direct calls (in tests)
pub trait SerialSyscall {
    /// Write bytes to serial output
    fn serial_write(&self, data: &[u8]) -> i32;

    /// Read bytes from serial input
    fn serial_read(&self, buf: &mut [u8]) -> i32;

    /// Check if serial input has data available
    fn serial_has_data(&self) -> bool;
}

/// Guest serial helper for line-based reading and buffering
pub struct GuestSerial<S: SerialSyscall> {
    syscall: S,
    buffer: VecDeque<u8>,
}

impl<S: SerialSyscall> GuestSerial<S> {
    /// Default buffer size for GuestSerial (32KB)
    pub const DEFAULT_BUF_SIZE: usize = 32 * 1024;

    /// Create a new GuestSerial instance with default buffer size
    pub fn new(syscall: S) -> Self {
        Self::new_with_capacity(syscall, Self::DEFAULT_BUF_SIZE)
    }

    /// Create a new GuestSerial instance with specified buffer capacity
    pub fn new_with_capacity(syscall: S, buffer_capacity: usize) -> Self {
        GuestSerial {
            syscall,
            buffer: VecDeque::with_capacity(buffer_capacity),
        }
    }

    /// Read a line from serial (until newline or EOF)
    /// Fills internal buffer by calling syscall in a loop
    pub fn read_line(&mut self) -> String {
        // First, try to read from existing buffer
        if let Some(newline_pos) = self.buffer.iter().position(|&b| b == b'\n' || b == b'\r') {
            let mut line = String::new();
            for _ in 0..=newline_pos {
                if let Some(byte) = self.buffer.pop_front()
                    && byte != b'\n'
                    && byte != b'\r'
                {
                    line.push(byte as char);
                }
            }
            return line;
        }

        // Buffer doesn't have a complete line, read more from syscall
        let mut temp_buf = [0u8; 64];
        loop {
            if !self.syscall.serial_has_data() {
                // No more data, return what we have
                break;
            }

            let bytes_read = self.syscall.serial_read(&mut temp_buf);
            if bytes_read <= 0 {
                break;
            }

            let bytes_read = bytes_read as usize;
            for (i, &byte) in temp_buf[..bytes_read].iter().enumerate() {
                if byte == b'\n' || byte == b'\r' {
                    // Found newline, build line from buffer and return
                    let mut line = String::new();
                    while let Some(b) = self.buffer.pop_front() {
                        line.push(b as char);
                    }
                    // Push remaining bytes after newline back to buffer for next read_line()
                    for &remaining_byte in &temp_buf[i + 1..bytes_read] {
                        self.buffer.push_back(remaining_byte);
                    }
                    // Don't push the newline byte to buffer
                    return line;
                }
                self.buffer.push_back(byte);
            }

            // Continue loop to read more if no newline found
        }

        // No newline found, return what we have
        let mut line = String::new();
        while let Some(byte) = self.buffer.pop_front() {
            line.push(byte as char);
        }
        line
    }

    /// Write bytes to serial
    pub fn write(&mut self, data: &[u8]) -> i32 {
        self.syscall.serial_write(data)
    }

    /// Write a line to serial (appends newline)
    ///
    /// # Arguments
    /// * `line` - Line to write (without newline)
    ///
    /// # Returns
    /// * Positive number: bytes written (including newline)
    /// * Negative number: error code
    pub fn write_line(&mut self, line: &str) -> i32 {
        let mut data = alloc::vec::Vec::with_capacity(line.len() + 1);
        data.extend_from_slice(line.as_bytes());
        data.push(b'\n');
        self.syscall.serial_write(&data)
    }
}
