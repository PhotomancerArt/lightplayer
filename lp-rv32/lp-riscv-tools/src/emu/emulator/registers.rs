//! Register, PC, and memory accessor methods.

use super::super::memory::Memory;
use super::state::Riscv32Emulator;
use crate::Gpr;

impl Riscv32Emulator {
    /// Get the value of a register.
    pub fn get_register(&self, reg: Gpr) -> i32 {
        if reg.num() == 0 {
            0
        } else {
            self.regs[reg.num() as usize]
        }
    }

    /// Set the value of a register.
    ///
    /// Note: Writing to x0 (ZERO) is a no-op.
    pub fn set_register(&mut self, reg: Gpr, value: i32) {
        if reg.num() != 0 {
            self.regs[reg.num() as usize] = value;
        }
    }

    /// Get the current program counter.
    pub fn get_pc(&self) -> u32 {
        self.pc
    }

    /// Set the program counter.
    pub fn set_pc(&mut self, pc: u32) {
        self.pc = pc;
    }

    /// Get a reference to the memory (for inspection).
    pub fn memory(&self) -> &Memory {
        &self.memory
    }

    /// Get a mutable reference to the memory (for initialization).
    pub fn memory_mut(&mut self) -> &mut Memory {
        &mut self.memory
    }
}
