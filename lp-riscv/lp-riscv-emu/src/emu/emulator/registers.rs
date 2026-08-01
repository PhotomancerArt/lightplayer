//! Register, PC, and memory accessor methods.

use super::super::fp_regs::FpRegs;
use super::state::Riscv32Emulator;
use lp_emu_core::Memory;
use lp_riscv_inst::Gpr;

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

    /// Read `f[index]` as a raw binary32 bit pattern (RV32F, FLEN = 32).
    ///
    /// Raw bits, never an `f32`: a signaling NaN or a NaN payload must survive
    /// inspection unchanged. Unlike `x0`, `f0` is an ordinary register.
    pub fn get_fp_register(&self, index: u8) -> u32 {
        self.fp.read_single(index)
    }

    /// Write a raw binary32 bit pattern to `f[index]`.
    pub fn set_fp_register(&mut self, index: u8, bits: u32) {
        self.fp.write_single(index, bits);
    }

    /// The full F-extension state (`f0`–`f31`, `fflags`, `frm`).
    pub fn fp_regs(&self) -> &FpRegs {
        &self.fp
    }

    /// Mutable access to the F-extension state, for test setup and hosts that
    /// need to seed or clear `fcsr`.
    pub fn fp_regs_mut(&mut self) -> &mut FpRegs {
        &mut self.fp
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
