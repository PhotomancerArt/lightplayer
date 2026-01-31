//! Syscall-based SerialIo implementation
//!
//! Uses emulator syscalls for serial I/O communication with the host.

extern crate alloc;

use alloc::vec::Vec;
use fw_core::serial::{SerialError, SerialIo};
use lp_riscv_emu_guest::syscall::{
    SYSCALL_ARGS, SYSCALL_SERIAL_HAS_DATA, SYSCALL_SERIAL_READ, SYSCALL_SERIAL_WRITE, syscall,
};

/// Syscall-based SerialIo implementation
///
/// Uses emulator syscalls to read/write serial data.
pub struct SyscallSerialIo;

impl SyscallSerialIo {
    /// Create a new syscall-based SerialIo instance
    pub fn new() -> Self {
        Self
    }
}

impl SerialIo for SyscallSerialIo {
    fn write(&mut self, data: &[u8]) -> Result<(), SerialError> {
        if data.is_empty() {
            return Ok(());
        }

        // Allocate buffer on heap and copy data
        let mut buffer = Vec::with_capacity(data.len());
        buffer.extend_from_slice(data);

        // Get pointer to buffer
        let ptr = buffer.as_ptr() as i32;
        let len = data.len() as i32;

        // Call syscall
        let mut args = [0i32; SYSCALL_ARGS];
        args[0] = ptr;
        args[1] = len;
        let result = syscall(SYSCALL_SERIAL_WRITE, &args);

        if result < 0 {
            Err(SerialError::WriteFailed(format!(
                "Syscall returned error: {}",
                result
            )))
        } else {
            Ok(())
        }
    }

    fn read_available(&mut self, buf: &mut [u8]) -> Result<usize, SerialError> {
        if buf.is_empty() {
            return Ok(0);
        }

        // Allocate buffer on heap
        let mut buffer = Vec::with_capacity(buf.len());
        buffer.resize(buf.len(), 0);

        // Get pointer to buffer
        let ptr = buffer.as_ptr() as i32;
        let max_len = buf.len() as i32;

        // Call syscall
        let mut args = [0i32; SYSCALL_ARGS];
        args[0] = ptr;
        args[1] = max_len;
        let result = syscall(SYSCALL_SERIAL_READ, &args);

        if result < 0 {
            Err(SerialError::ReadFailed(format!(
                "Syscall returned error: {}",
                result
            )))
        } else {
            let bytes_read = result as usize;
            // Copy data back
            let copy_len = bytes_read.min(buf.len());
            buf[..copy_len].copy_from_slice(&buffer[..copy_len]);
            Ok(bytes_read)
        }
    }

    fn has_data(&self) -> bool {
        let mut args = [0i32; SYSCALL_ARGS];
        let result = syscall(SYSCALL_SERIAL_HAS_DATA, &args);
        result != 0
    }
}
