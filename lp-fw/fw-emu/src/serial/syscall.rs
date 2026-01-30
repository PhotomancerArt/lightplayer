//! Syscall-based SerialIo implementation
//!
//! Uses emulator syscalls for serial I/O communication with the host.

use fw_core::serial::{SerialError, SerialIo};

/// Syscall-based SerialIo implementation
///
/// Uses emulator syscalls to read/write serial data.
/// TODO: Implement syscalls once emulator supports them.
pub struct SyscallSerialIo;

impl SyscallSerialIo {
    /// Create a new syscall-based SerialIo instance
    pub fn new() -> Self {
        Self
    }
}

impl SerialIo for SyscallSerialIo {
    fn write(&mut self, _data: &[u8]) -> Result<(), SerialError> {
        // TODO: Implement syscall for writing serial data
        todo!("Syscall-based serial write not yet implemented")
    }

    fn read_available(&mut self, _buf: &mut [u8]) -> Result<usize, SerialError> {
        // TODO: Implement syscall for reading serial data
        todo!("Syscall-based serial read not yet implemented")
    }

    fn has_data(&self) -> bool {
        // TODO: Implement syscall for checking if data is available
        todo!("Syscall-based serial has_data not yet implemented")
    }
}
