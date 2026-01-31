//! Syscall-based TimeProvider implementation
//!
//! Uses emulator syscalls to get time from the host.

use lp_riscv_emu_guest::syscall::{SYSCALL_ARGS, SYSCALL_TIME_MS, syscall};
use lp_shared::time::TimeProvider;

/// Syscall-based TimeProvider implementation
///
/// Uses emulator syscalls to get current time from the host.
pub struct SyscallTimeProvider;

impl SyscallTimeProvider {
    /// Create a new syscall-based TimeProvider instance
    pub fn new() -> Self {
        Self
    }
}

impl TimeProvider for SyscallTimeProvider {
    fn now_ms(&self) -> u64 {
        let mut args = [0i32; SYSCALL_ARGS];
        let result = syscall(SYSCALL_TIME_MS, &args);
        // Result is u32 milliseconds, cast to u64
        result as u64
    }
}
