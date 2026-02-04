//! Instruction executor for RISC-V 32-bit instructions.

extern crate alloc;

use crate::emu::{error::EmulatorError, logging::InstLog, memory::Memory};

/// Trait for compile-time logging mode control.
pub trait LoggingMode {
    /// Whether logging is enabled for this mode.
    const ENABLED: bool;
}

/// Logging enabled mode - creates InstLog entries.
pub struct LoggingEnabled;

impl LoggingMode for LoggingEnabled {
    const ENABLED: bool = true;
}

/// Logging disabled mode - zero logging overhead.
pub struct LoggingDisabled;

impl LoggingMode for LoggingDisabled {
    const ENABLED: bool = false;
}

/// Result of executing a single instruction.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// New PC value (None means PC += 4)
    pub new_pc: Option<u32>,
    /// Whether execution should stop (EBREAK)
    pub should_halt: bool,
    /// Whether a syscall was encountered (ECALL)
    pub syscall: bool,
    /// Log entry for this instruction (None if logging is disabled)
    pub log: Option<InstLog>,
}

/// Helper to read register (x0 always returns 0)
#[inline(always)]
pub(super) fn read_reg(regs: &[i32; 32], reg: lp_riscv_inst::Gpr) -> i32 {
    if reg.num() == 0 {
        0
    } else {
        regs[reg.num() as usize]
    }
}

/// Main dispatch function for decode-execute fusion.
///
/// Decodes the instruction word and executes it in a single step,
/// eliminating the intermediate `Inst` enum allocation.
pub fn decode_execute<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    _memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    // Check if compressed instruction (bits [1:0] != 0b11)
    if (inst_word & 0x3) != 0x3 {
        return compressed::decode_execute_compressed::<M>(inst_word, pc, regs, _memory);
    }

    let opcode = (inst_word & 0x7f) as u8;

    match opcode {
        0x33 => {
            // R-type (arithmetic)
            arithmetic::decode_execute_rtype::<M>(inst_word, pc, regs, _memory)
        }
        0x13 => {
            // I-type (immediate arithmetic/logical/shift)
            immediate::decode_execute_itype::<M>(inst_word, pc, regs, _memory)
        }
        0x03 => {
            // Load instructions
            load_store::decode_execute_load::<M>(inst_word, pc, regs, _memory)
        }
        0x23 => {
            // Store instructions
            load_store::decode_execute_store::<M>(inst_word, pc, regs, _memory)
        }
        0x63 => {
            // Branch instructions
            branch::decode_execute_branch::<M>(inst_word, pc, regs, _memory)
        }
        0x6f => {
            // JAL
            jump::decode_execute_jal::<M>(inst_word, pc, regs, _memory)
        }
        0x67 => {
            // JALR
            jump::decode_execute_jalr::<M>(inst_word, pc, regs, _memory)
        }
        0x37 => {
            // LUI
            jump::decode_execute_lui::<M>(inst_word, pc, regs, _memory)
        }
        0x17 => {
            // AUIPC
            jump::decode_execute_auipc::<M>(inst_word, pc, regs, _memory)
        }
        0x73 => {
            // System instructions (ECALL, EBREAK, CSR)
            system::decode_execute_system::<M>(inst_word, pc, regs, _memory)
        }
        0x0f => {
            // FENCE/FENCE.I instructions
            system::decode_execute_fence::<M>(inst_word, pc, regs, _memory)
        }
        0x2f => {
            // Atomic instructions (A extension)
            atomic::decode_execute_atomic::<M>(inst_word, pc, regs, _memory)
        }
        _ => Err(EmulatorError::InvalidInstruction {
            pc,
            instruction: inst_word,
            reason: alloc::format!("Unknown opcode: 0x{opcode:02x}"),
            regs: *regs,
        }),
    }
}

// Category modules
pub mod arithmetic;
pub mod atomic;
pub mod branch;
pub mod compressed;
pub mod immediate;
pub mod jump;
pub mod load_store;
pub mod system;
