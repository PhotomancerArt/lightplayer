//! Syscall-based OutputProvider implementation
//!
//! Uses emulator syscalls to send LED output data to the host.

extern crate alloc;

use alloc::{rc::Rc, vec::Vec};
use core::cell::RefCell;

use lp_shared::OutputError;
use lp_shared::output::{OutputChannelHandle, OutputFormat, OutputProvider};

/// Syscall-based OutputProvider implementation
///
/// Uses emulator syscalls to send LED output data to the host for display/visualization.
/// TODO: Implement syscalls once emulator supports them.
pub struct SyscallOutputProvider {
    // TODO: Add state as needed
}

impl SyscallOutputProvider {
    /// Create a new syscall-based OutputProvider instance
    pub fn new() -> Self {
        Self {}
    }
}

impl OutputProvider for SyscallOutputProvider {
    fn open(
        &self,
        _pin: u32,
        _byte_count: u32,
        _format: OutputFormat,
    ) -> Result<OutputChannelHandle, OutputError> {
        // TODO: Implement syscall for opening output channel
        todo!("Syscall-based output open not yet implemented")
    }

    fn write(&self, _handle: OutputChannelHandle, _data: &[u8]) -> Result<(), OutputError> {
        // TODO: Implement syscall for writing LED data to host
        todo!("Syscall-based output write not yet implemented")
    }

    fn close(&self, _handle: OutputChannelHandle) -> Result<(), OutputError> {
        // TODO: Implement syscall for closing output channel
        todo!("Syscall-based output close not yet implemented")
    }
}
