# Phase 3: Implement SerialHost with Comprehensive Tests

## Scope of phase

Complete the `SerialHost` implementation in `lp-riscv-tools/src/emu/serial_host.rs` with all
methods, proper error handling, and comprehensive tests.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Complete `SerialHost` struct

```rust
use alloc::collections::VecDeque;
use lp_riscv_emu_shared::{
    SERIAL_ERROR_BUFFER_FULL, SERIAL_ERROR_BUFFER_NOT_ALLOCATED,
};

pub struct SerialHost {
    to_guest_buf: VecDeque<u8>,      // Host → Guest (guest reads from this)
    from_guest_buf: VecDeque<u8>,   // Guest → Host (guest writes to this)
}

impl SerialHost {
    pub fn new(buffer_size: usize) -> Self {
        SerialHost {
            to_guest_buf: VecDeque::with_capacity(buffer_size),
            from_guest_buf: VecDeque::with_capacity(buffer_size),
        }
    }

    /// Handles guest writing data to host
    /// Called by the handler for SYSCALL_SERIAL_WRITE
    /// Returns: bytes written (positive) or error code (negative)
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
        }

        to_write as i32
    }

    /// Handles guest reading data from host
    /// Called by the handler for SYSCALL_SERIAL_READ
    /// Returns: bytes read (positive) or error code (negative)
    pub fn guest_read(&mut self, buffer: &mut [u8]) -> i32 {
        if buffer.is_empty() {
            return 0;
        }

        if self.to_guest_buf.is_empty() {
            return 0;
        }

        // Read available bytes (up to buffer length)
        let to_read = buffer.len().min(self.to_guest_buf.len());
        for i in 0..to_read {
            if let Some(byte) = self.to_guest_buf.pop_front() {
                buffer[i] = byte;
            } else {
                return i as i32;
            }
        }

        to_read as i32
    }

    /// Check if guest has data available to read
    /// Called by the handler for SYSCALL_SERIAL_HAS_DATA
    pub fn has_data(&self) -> bool {
        !self.to_guest_buf.is_empty()
    }

    /// Handles host writing data to guest
    /// Called by the user of the emulator to send data to the guest
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
            self.to_guest_buf.extend(&buffer[..to_write]);
        }

        Ok(to_write)
    }

    /// Handles host reading data from guest
    /// Called by the user of the emulator to read data from the guest
    pub fn host_read(&mut self, buffer: &mut [u8]) -> Result<usize, SerialError> {
        if buffer.is_empty() {
            return Ok(0);
        }

        if self.from_guest_buf.is_empty() {
            return Ok(0);
        }

        // Read available bytes (up to buffer length)
        let to_read = buffer.len().min(self.from_guest_buf.len());
        for i in 0..to_read {
            if let Some(byte) = self.from_guest_buf.pop_front() {
                buffer[i] = byte;
            } else {
                return Ok(i);
            }
        }

        Ok(to_read)
    }
}

// Define SerialError for host-side use
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SerialError {
    BufferFull,
    // Add more as needed
}

impl core::fmt::Display for SerialError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SerialError::BufferFull => write!(f, "Serial buffer full"),
        }
    }
}
```

### 2. Write comprehensive tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guest_write_basic() {
        let mut serial = SerialHost::new(128 * 1024);
        let data = b"hello";
        let result = serial.guest_write(data);
        assert_eq!(result, 5);
        assert_eq!(serial.from_guest_buf.len(), 5);
    }

    #[test]
    fn test_guest_read_basic() {
        let mut serial = SerialHost::new(128 * 1024);
        serial.to_guest_buf.extend(b"hello");
        let mut buf = [0u8; 10];
        let result = serial.guest_read(&mut buf);
        assert_eq!(result, 5);
        assert_eq!(&buf[..5], b"hello");
        assert!(serial.to_guest_buf.is_empty());
    }

    #[test]
    fn test_buffer_size_limit() {
        let mut serial = SerialHost::new(128 * 1024);
        // Fill buffer to limit
        let large_data = vec![0u8; 128 * 1024];
        let result = serial.guest_write(&large_data);
        assert_eq!(result, 128 * 1024);

        // Try to write more - should return error
        let result = serial.guest_write(b"extra");
        assert_eq!(result, SERIAL_ERROR_BUFFER_FULL);
    }

    #[test]
    fn test_fifo_behavior() {
        let mut serial = SerialHost::new(128 * 1024);
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
        let mut serial = SerialHost::new(128 * 1024);
        assert!(!serial.has_data());
        serial.to_guest_buf.extend(b"test");
        assert!(serial.has_data());
    }

    #[test]
    fn test_partial_read() {
        let mut serial = SerialHost::new(128 * 1024);
        serial.to_guest_buf.extend(b"hello world");
        let mut buf = [0u8; 5];
        let result = serial.guest_read(&mut buf);
        assert_eq!(result, 5);
        assert_eq!(&buf, b"hello");
        assert_eq!(serial.to_guest_buf.len(), 6); // " world" remaining
    }

    #[test]
    fn test_host_write_read() {
        let mut serial = SerialHost::new(128 * 1024);
        let data = b"test data";
        let result = serial.host_write(data);
        assert_eq!(result, Ok(9));

        let mut buf = [0u8; 20];
        let result = serial.host_read(&mut buf);
        assert_eq!(result, Ok(9));
        assert_eq!(&buf[..9], data);
    }

    #[test]
    fn test_empty_read() {
        let mut serial = SerialHost::new(128 * 1024);
        let mut buf = [0u8; 10];
        let result = serial.guest_read(&mut buf);
        assert_eq!(result, 0);
    }
}
```

## Validate

Run from workspace root:

```bash
cargo test --package lp-riscv-tools --lib serial_host
cargo check --package lp-riscv-tools
```

Ensure:

- All tests pass
- Code compiles without errors
- No warnings
- Buffer size limits are enforced
- FIFO behavior is correct
- Error cases are handled properly
