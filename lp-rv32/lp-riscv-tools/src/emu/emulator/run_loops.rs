//! High-level run loop methods.

extern crate alloc;

use super::super::error::EmulatorError;
use super::state::Riscv32Emulator;
use super::types::{StepResult, SyscallInfo};
use crate::Gpr;
use alloc::string::String;

impl Riscv32Emulator {
    /// Run until EBREAK is encountered, returning the value in a0.
    pub fn run_until_ebreak(&mut self) -> Result<i32, EmulatorError> {
        loop {
            match self.step()? {
                StepResult::Halted => {
                    return Ok(self.regs[Gpr::A0.num() as usize]);
                }
                StepResult::Trap(code) => {
                    // Trap encountered - return error
                    return Err(EmulatorError::Trap {
                        code,
                        pc: self.pc,
                        regs: self.regs,
                    });
                }
                StepResult::Panic(info) => {
                    // Panic encountered - return error
                    return Err(EmulatorError::Panic {
                        info,
                        pc: self.pc,
                        regs: self.regs,
                    });
                }
                StepResult::Continue => {
                    // Continue execution
                }
                StepResult::Syscall(_) => {
                    // Treat syscall as error in this context (caller should use run_until_ecall)
                    return Err(EmulatorError::InvalidInstruction {
                        pc: self.pc,
                        instruction: 0,
                        reason: String::from("Unexpected ECALL in run_until_ebreak"),
                        regs: self.regs,
                    });
                }
            }
        }
    }

    /// Run until ECALL is encountered, returning syscall information.
    pub fn run_until_ecall(&mut self) -> Result<SyscallInfo, EmulatorError> {
        loop {
            match self.step()? {
                StepResult::Syscall(info) => {
                    return Ok(info);
                }
                StepResult::Halted => {
                    return Err(EmulatorError::InvalidInstruction {
                        pc: self.pc,
                        instruction: 0,
                        reason: String::from("Unexpected EBREAK in run_until_ecall"),
                        regs: self.regs,
                    });
                }
                StepResult::Trap(_) => {
                    return Err(EmulatorError::InvalidInstruction {
                        pc: self.pc,
                        instruction: 0,
                        reason: String::from("Unexpected trap in run_until_ecall"),
                        regs: self.regs,
                    });
                }
                StepResult::Panic(info) => {
                    return Err(EmulatorError::Panic {
                        info,
                        pc: self.pc,
                        regs: self.regs,
                    });
                }
                StepResult::Continue => {
                    // Continue execution
                }
            }
        }
    }
}
