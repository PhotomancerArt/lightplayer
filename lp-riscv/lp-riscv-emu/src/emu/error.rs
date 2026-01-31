//! Error types for the RISC-V 32 emu.

extern crate alloc;

use alloc::string::String;

use cranelift_codegen::ir::TrapCode;
use lp_riscv_inst::Gpr;

/// Kind of memory access that failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryAccessKind {
    Read,
    Write,
    InstructionFetch,
}

/// Errors that can occur during emulation.
#[derive(Debug, Clone)]
pub enum EmulatorError {
    /// Instruction limit exceeded.
    InstructionLimitExceeded {
        limit: u64,
        executed: u64,
        pc: u32,
        regs: [i32; 32],
    },
    /// Invalid memory access (out of bounds).
    InvalidMemoryAccess {
        address: u32,
        size: usize,
        kind: MemoryAccessKind,
        pc: u32,
        regs: [i32; 32],
    },
    /// Invalid instruction encoding.
    InvalidInstruction {
        pc: u32,
        instruction: u32,
        reason: String,
        regs: [i32; 32],
    },
    /// Unaligned memory access.
    UnalignedAccess {
        address: u32,
        alignment: usize,
        pc: u32,
        regs: [i32; 32],
    },
    /// Unknown or unsupported opcode.
    UnknownOpcode {
        opcode: u8,
        pc: u32,
        instruction: u32,
        regs: [i32; 32],
    },
    /// Invalid register access.
    InvalidRegister { reg: Gpr, pc: u32, reason: String },
    /// Trap encountered during execution.
    Trap {
        code: TrapCode,
        pc: u32,
        regs: [i32; 32],
    },
    /// Panic occurred in the emulated program.
    Panic {
        info: super::PanicInfo,
        pc: u32,
        regs: [i32; 32],
    },
}

/// Convert a TrapCode to a human-readable string.
pub fn trap_code_to_string(code: TrapCode) -> &'static str {
    match code {
        TrapCode::STACK_OVERFLOW => "stack overflow",
        TrapCode::INTEGER_OVERFLOW => "integer overflow",
        TrapCode::HEAP_OUT_OF_BOUNDS => "heap out of bounds",
        TrapCode::INTEGER_DIVISION_BY_ZERO => "integer division by zero",
        TrapCode::BAD_CONVERSION_TO_INTEGER => "bad conversion to integer",
        _ => {
            // Check for user-defined trap codes
            let raw = code.as_raw().get();
            if raw == 1 {
                "vector/matrix index out of bounds"
            } else {
                "unknown trap"
            }
        }
    }
}

impl EmulatorError {
    /// Get the PC where the error occurred.
    pub fn pc(&self) -> u32 {
        match self {
            EmulatorError::InstructionLimitExceeded { pc, .. } => *pc,
            EmulatorError::InvalidMemoryAccess { pc, .. } => *pc,
            EmulatorError::InvalidInstruction { pc, .. } => *pc,
            EmulatorError::UnalignedAccess { pc, .. } => *pc,
            EmulatorError::UnknownOpcode { pc, .. } => *pc,
            EmulatorError::InvalidRegister { pc, .. } => *pc,
            EmulatorError::Trap { pc, .. } => *pc,
            EmulatorError::Panic { pc, .. } => *pc,
        }
    }

    /// Get a snapshot of register state at the time of error.
    pub fn regs(&self) -> Option<&[i32; 32]> {
        match self {
            EmulatorError::InstructionLimitExceeded { regs, .. } => Some(regs),
            EmulatorError::InvalidMemoryAccess { regs, .. } => Some(regs),
            EmulatorError::InvalidInstruction { regs, .. } => Some(regs),
            EmulatorError::UnalignedAccess { regs, .. } => Some(regs),
            EmulatorError::UnknownOpcode { regs, .. } => Some(regs),
            EmulatorError::InvalidRegister { .. } => None,
            EmulatorError::Trap { regs, .. } => Some(regs),
            EmulatorError::Panic { regs, .. } => Some(regs),
        }
    }
}

impl core::fmt::Display for EmulatorError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EmulatorError::InstructionLimitExceeded {
                limit,
                executed,
                pc,
                ..
            } => write!(
                f,
                "Instruction limit exceeded: executed {executed} instructions (limit: {limit}) at PC 0x{pc:08x}"
            ),
            EmulatorError::InvalidMemoryAccess {
                address,
                size,
                kind,
                pc,
                ..
            } => {
                let kind_str = match kind {
                    MemoryAccessKind::Read => "read",
                    MemoryAccessKind::Write => "write",
                    MemoryAccessKind::InstructionFetch => "instruction fetch",
                };
                write!(
                    f,
                    "Invalid memory {kind_str} at address 0x{address:08x} (size: {size} bytes) at PC 0x{pc:08x}"
                )
            }
            EmulatorError::InvalidInstruction {
                pc,
                instruction,
                reason,
                ..
            } => write!(
                f,
                "Invalid instruction 0x{instruction:08x} at PC 0x{pc:08x}: {reason}"
            ),
            EmulatorError::UnalignedAccess {
                address,
                alignment,
                pc,
                ..
            } => write!(
                f,
                "Unaligned memory access at address 0x{address:08x} (requires {alignment} byte alignment) at PC \
                 0x{pc:08x}"
            ),
            EmulatorError::UnknownOpcode {
                opcode,
                pc,
                instruction,
                ..
            } => write!(
                f,
                "Unknown opcode 0x{opcode:02x} in instruction 0x{instruction:08x} at PC 0x{pc:08x}"
            ),
            EmulatorError::InvalidRegister { reg, pc, reason } => write!(
                f,
                "Invalid register access: {reg:?} at PC 0x{pc:08x}: {reason}"
            ),
            EmulatorError::Trap { code, pc, .. } => {
                let trap_name = trap_code_to_string(*code);
                write!(f, "Trap: {trap_name} at PC 0x{pc:08x}")
            }
            EmulatorError::Panic { info, pc, .. } => {
                write!(f, "Panic at PC 0x{:08x}: {}", pc, info.message)?;
                if let Some(ref file) = info.file {
                    write!(f, "\n  at {file}")?;
                    if let Some(line) = info.line {
                        write!(f, ":{line}")?;
                    }
                } else if let Some(line) = info.line {
                    write!(f, "\n  at line {line}")?;
                }
                Ok(())
            }
        }
    }
}

// Note: std::error::Error trait implementation would require std feature
