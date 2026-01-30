//! Core state and initialization for the RISC-V 32-bit emulator.

extern crate alloc;

use super::super::{logging::LogLevel, memory::Memory};
use alloc::vec::Vec;
use cranelift_codegen::ir::TrapCode;

/// Default RAM start address (0x80000000, matching embive's RAM_OFFSET).
pub const DEFAULT_RAM_START: u32 = 0x80000000;

/// RISC-V 32-bit emulator state.
pub struct Riscv32Emulator {
    pub(super) regs: [i32; 32],
    pub(super) pc: u32,
    pub(super) memory: Memory,
    pub(super) instruction_count: u64,
    pub(super) max_instructions: u64,
    pub(super) log_level: LogLevel,
    pub(super) log_buffer: Vec<super::super::logging::InstLog>,
    pub(super) traps: Vec<(u32, TrapCode)>, // sorted by offset (offset, trap_code) pairs
}

impl Riscv32Emulator {
    /// Create a new emulator with the given code, RAM, and trap information.
    ///
    /// # Arguments
    ///
    /// * `code` - Code region (instructions)
    /// * `ram` - RAM region (data)
    /// * `traps` - Trap information from compiled code (offset -> TrapCode pairs)
    pub fn with_traps(code: Vec<u8>, ram: Vec<u8>, traps: &[(u32, TrapCode)]) -> Self {
        // Sort traps by offset for efficient binary search lookup
        let mut trap_list: Vec<(u32, TrapCode)> = traps.to_vec();
        trap_list.sort_by_key(|(offset, _)| *offset);

        Self {
            regs: [0; 32],
            pc: 0,
            memory: Memory::with_default_addresses(code, ram),
            instruction_count: 0,
            max_instructions: 100_000,
            log_level: LogLevel::None,
            log_buffer: Vec::new(),
            traps: trap_list,
        }
    }

    /// Create a new emulator with the given code and RAM.
    ///
    /// # Arguments
    ///
    /// * `code` - Code region (instructions)
    /// * `ram` - RAM region (data)
    pub fn new(code: Vec<u8>, ram: Vec<u8>) -> Self {
        Self::with_traps(code, ram, &[])
    }

    /// Set the maximum number of instructions to execute.
    pub fn with_max_instructions(mut self, limit: u64) -> Self {
        self.max_instructions = limit;
        self
    }

    /// Set the maximum number of instructions to execute (mutating method).
    pub fn set_max_instructions(&mut self, limit: u64) {
        self.max_instructions = limit;
    }

    /// Set the logging level.
    pub fn with_log_level(mut self, level: LogLevel) -> Self {
        self.log_level = level;
        self
    }

    /// Get the number of instructions executed so far.
    pub fn get_instruction_count(&self) -> u64 {
        self.instruction_count
    }
}
