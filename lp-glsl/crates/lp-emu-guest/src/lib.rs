#![no_std]

//! RISC-V32 emulator guest runtime library
//!
//! This crate provides the runtime foundation for code running in the RISC-V emulator.
//! It includes:
//! - Entry point and bootstrap code
//! - Panic handler with syscall reporting
//! - Host communication functions
//! - Print macros for no_std environments

pub mod entry;
pub mod host;
pub mod panic;
pub mod print;

mod syscall;

// Re-export ebreak function for convenience
pub use panic::ebreak;
