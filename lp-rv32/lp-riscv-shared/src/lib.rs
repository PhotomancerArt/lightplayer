//! Shared code for RISC-V JIT testing using Cranelift.
//!
//! This crate provides common functionality for building and compiling
//! toy language code to RISC-V that can be used both in the embive VM and on real hardware.

#![no_std]

extern crate alloc;

pub mod guest_serial;
mod simple_elf;
mod syscall;

pub use syscall::{
    SYSCALL_ARGS, SYSCALL_DEBUG, SYSCALL_PANIC, SYSCALL_SERIAL_HAS_DATA, SYSCALL_SERIAL_READ,
    SYSCALL_SERIAL_WRITE, SYSCALL_TIME_MS, SYSCALL_WRITE, SYSCALL_YIELD,
};

pub use guest_serial::{
    GuestSerial, SERIAL_ERROR_BUFFER_FULL, SERIAL_ERROR_BUFFER_NOT_ALLOCATED,
    SERIAL_ERROR_INVALID_POINTER, SerialSyscall,
};
