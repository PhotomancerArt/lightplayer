//! Syscall-based TimeProvider implementation
//!
//! Uses emulator syscalls to get time from the host.

use lp_shared::time::TimeProvider;

/// Syscall-based TimeProvider implementation
///
/// Uses emulator syscalls to get current time from the host.
/// TODO: Implement syscalls once emulator supports them.
pub struct SyscallTimeProvider;

impl SyscallTimeProvider {
    /// Create a new syscall-based TimeProvider instance
    pub fn new() -> Self {
        Self
    }
}

impl TimeProvider for SyscallTimeProvider {
    fn now_ms(&self) -> u64 {
        // TODO: Implement syscall to get current time from host
        todo!("Syscall-based time not yet implemented")
    }
}
