//! Shared code between RISC-V emulator host and guest.
//!
//! This crate provides common definitions and types shared between the emulator runtime
//! and code running inside the emulator, including syscall constants and serial communication types.

#![no_std]

extern crate alloc;

pub mod guest_serial;
mod simple_elf;
mod syscall;

pub use syscall::{
    SYSCALL_ARGS, SYSCALL_LOG, SYSCALL_PANIC, SYSCALL_SERIAL_HAS_DATA, SYSCALL_SERIAL_READ,
    SYSCALL_SERIAL_WRITE, SYSCALL_TIME_MS, SYSCALL_WRITE, SYSCALL_YIELD, level_to_syscall,
    syscall_to_level,
};

pub use guest_serial::{
    GuestSerial, SERIAL_ERROR_BUFFER_FULL, SERIAL_ERROR_BUFFER_NOT_ALLOCATED,
    SERIAL_ERROR_INVALID_POINTER, SerialSyscall,
};
