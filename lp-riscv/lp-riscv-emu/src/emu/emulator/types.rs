//! Public types for the RISC-V 32-bit emulator.

extern crate alloc;

use alloc::string::String;
use cranelift_codegen::ir::TrapCode;

/// Result of a single step.
#[derive(Debug, Clone)]
pub enum StepResult {
    /// Normal step completed, continue execution
    Continue,
    /// ECALL encountered, syscall information available
    Syscall(SyscallInfo),
    /// EBREAK encountered, execution halted
    Halted,
    /// Trap encountered with trap code
    Trap(TrapCode),
    /// Panic occurred, panic information available
    Panic(PanicInfo),
    /// Fuel exhausted during run (instructions executed in this run)
    /// Only returned by run() functions, never by step()
    FuelExhausted(u64),
}

/// Information about a syscall (ECALL).
#[derive(Debug, Clone)]
pub struct SyscallInfo {
    /// Syscall number (from a7 register)
    pub number: i32,
    /// Syscall arguments (from a0-a6 registers)
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
