//! Syscall-based SerialIo implementation
//!
//! Uses emulator syscalls for serial I/O communication with the host.

use alloc::format;
use fw_core::serial::{SerialError, SerialIo};
use log;
use lp_riscv_emu_guest::{sys_serial_has_data, sys_serial_read, sys_serial_write};

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
        let result = sys_serial_write(data);
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
        let result = sys_serial_read(buf);
        log::trace!(
            "SyscallSerialIo::read_available: sys_serial_read returned {}, buf.len()={}",
            result,
            buf.len()
        );
        if result < 0 {
            Err(SerialError::ReadFailed(format!(
                "Syscall returned error: {}",
                result
            )))
        } else {
            Ok(result as usize)
        }
    }

    fn has_data(&self) -> bool {
        sys_serial_has_data()
    }
}
