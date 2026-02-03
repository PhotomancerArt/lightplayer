//! Syscall-based OutputProvider implementation
//!
//! Uses emulator syscalls to send LED output data to the host.

extern crate alloc;

use alloc::vec::Vec;
use core::cell::RefCell;

use lp_riscv_emu_guest::println;
use lp_shared::OutputError;
use lp_shared::output::{OutputChannelHandle, OutputFormat, OutputProvider};

/// Syscall-based OutputProvider implementation
///
/// For now, uses print logging to indicate output changes.
/// Output syscalls will be added later if needed.
pub struct SyscallOutputProvider {
    handles: RefCell<Vec<OutputChannelHandle>>,
    next_handle: RefCell<u32>,
}

impl SyscallOutputProvider {
    /// Create a new syscall-based OutputProvider instance
    pub fn new() -> Self {
        Self {
            handles: RefCell::new(Vec::new()),
            next_handle: RefCell::new(1),
        }
    }
}

impl OutputProvider for SyscallOutputProvider {
    fn open(
        &self,
        pin: u32,
        byte_count: u32,
        format: OutputFormat,
    ) -> Result<OutputChannelHandle, OutputError> {
        let handle_id = *self.next_handle.borrow();
        *self.next_handle.borrow_mut() += 1;
        let handle = OutputChannelHandle::new(handle_id as i32);
        self.handles.borrow_mut().push(handle);

        println!(
            "[output] open: pin={}, bytes={}, format={:?}, handle={:?}",
            pin, byte_count, format, handle
        );

        Ok(handle)
    }

    fn write(&self, handle: OutputChannelHandle, data: &[u8]) -> Result<(), OutputError> {
        println!("[output] write: handle={:?}, len={}", handle, data.len());
        // TODO: Implement syscall for writing LED data to host
        // For now, just succeed
        Ok(())
    }

    fn close(&self, handle: OutputChannelHandle) -> Result<(), OutputError> {
        println!("[output] close: handle={:?}", handle);
        // TODO: Implement syscall for closing output channel
        // For now, just succeed
        Ok(())
    }
}
