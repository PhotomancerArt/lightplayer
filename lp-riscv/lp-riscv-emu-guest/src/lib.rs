//! RISC-V 32-bit emulator guest runtime library.
//!
//! This crate provides the runtime foundation for code running inside the RISC-V emulator.
//! It includes:
//! - Entry point and bootstrap code
//! - Panic handler with syscall reporting
//! - Host communication functions and syscall wrappers
//! - Print macros for no_std environments
//! - Heap allocator for dynamic allocation

#![no_std]

pub mod allocator;
pub mod entry;
pub mod host;
pub mod panic;
pub mod print;

mod syscall;

// Re-export syscall constants and function
pub use syscall::{
    SYSCALL_ARGS, SYSCALL_DEBUG, SYSCALL_PANIC, SYSCALL_SERIAL_HAS_DATA, SYSCALL_SERIAL_READ,
    SYSCALL_SERIAL_WRITE, SYSCALL_TIME_MS, SYSCALL_WRITE, SYSCALL_YIELD, sys_serial_has_data,
    sys_serial_read, sys_serial_write, syscall,
};

// Re-export ebreak function for convenience
pub use panic::ebreak;

// Re-export _print function for convenience (macros are already exported at crate root via #[macro_export])
pub use print::_print;

// Re-export guest serial types from shared crate
pub use lp_riscv_emu_shared::{GuestSerial, SerialSyscall};

/// Implementation for actual guest syscalls
/// This is guest-specific, so it stays in lp-riscv-emu-guest
pub struct GuestSyscallImpl;

impl SerialSyscall for GuestSyscallImpl {
    fn serial_write(&self, data: &[u8]) -> i32 {
        syscall::sys_serial_write(data)
    }

    fn serial_read(&self, buf: &mut [u8]) -> i32 {
        syscall::sys_serial_read(buf)
    }

    fn serial_has_data(&self) -> bool {
        syscall::sys_serial_has_data()
    }
}
