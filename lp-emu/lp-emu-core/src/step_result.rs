//! Public result types for emulator run loops.
//!
//! These form the arch-neutral run-loop contract shared by LightPlayer
//! emulators: a single step (or fueled run) yields a [`StepResult`], and the
//! host reacts to syscalls, traps, panics, and fuel exhaustion.

extern crate alloc;

use alloc::string::String;

use crate::trap_code::TrapCode;

/// Result of a single step.
#[derive(Debug, Clone)]
pub enum StepResult {
    /// Normal step completed, continue execution
    Continue,
    /// Syscall encountered, syscall information available
    Syscall(SyscallInfo),
    /// Breakpoint/halt instruction encountered, execution halted
    Halted,
    /// Trap encountered with trap code
    Trap(TrapCode),
    /// Panic occurred, panic information available
    Panic(PanicInfo),
    /// Out of memory — guest allocation failed
    Oom(OomInfo),
    /// Fuel exhausted during run (instructions executed in this run)
    /// Only returned by run() functions, never by step()
    FuelExhausted(u64),
    /// The active profile session's gate requested termination.
    /// The CLI should drain remaining buffers and finish the session.
    ProfileStop,
}

/// Information about an out-of-memory condition.
#[derive(Debug, Clone)]
pub struct OomInfo {
    /// Size of the allocation that failed
    pub size: u32,
    /// Program counter where OOM occurred
    pub pc: u32,
}

/// Information about a syscall.
#[derive(Debug, Clone)]
pub struct SyscallInfo {
    /// Syscall number
    pub number: i32,
    /// Syscall arguments (`SYSCALL_ARGS` = 7 words in the shared protocol)
    pub args: [i32; 7],
}

/// Information about a panic that occurred in the emulated program.
#[derive(Debug, Clone)]
pub struct PanicInfo {
    /// Panic message
    pub message: String,
    /// Source file name (if available)
    pub file: Option<String>,
    /// Line number (if available)
    pub line: Option<u32>,
    /// Program counter where panic occurred
    pub pc: u32,
}
