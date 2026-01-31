//! High-level run loop methods.

extern crate alloc;

use super::super::error::EmulatorError;
use super::state::Riscv32Emulator;
use super::types::{StepResult, SyscallInfo};
use alloc::string::String;
use lp_riscv_emu_shared::SYSCALL_YIELD;
use lp_riscv_inst::Gpr;

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

    /// Step until a yield syscall is encountered, with a maximum step limit
    ///
    /// Steps the emulator until a yield syscall (SYSCALL_YIELD) is encountered,
    /// or until the maximum number of steps is reached.
    ///
    /// # Arguments
    /// * `max_steps` - Maximum number of steps to execute
    ///
    /// # Returns
    /// * `Ok(SyscallInfo)` - Yield syscall was encountered
    /// * `Err(EmulatorError)` - Error occurred (trap, panic, instruction limit, or max steps exceeded)
    pub fn step_until_yield(&mut self, max_steps: u64) -> Result<SyscallInfo, EmulatorError> {
        let initial_instruction_count = self.instruction_count;
        let max_instructions = initial_instruction_count + max_steps;

        loop {
            // Check if we've exceeded the step limit
            if self.instruction_count >= max_instructions {
                return Err(EmulatorError::InstructionLimitExceeded {
                    limit: max_instructions,
                    executed: self.instruction_count,
                    pc: self.pc,
                    regs: self.regs,
                });
            }

            match self.step()? {
                StepResult::Syscall(info) if info.number == SYSCALL_YIELD => {
                    return Ok(info);
                }
                StepResult::Syscall(_) => {
                    // Other syscall - continue execution
                }
                StepResult::Halted => {
                    return Err(EmulatorError::InvalidInstruction {
                        pc: self.pc,
                        instruction: 0,
                        reason: String::from("Unexpected EBREAK in step_until_yield"),
                        regs: self.regs,
                    });
                }
                StepResult::Trap(code) => {
                    return Err(EmulatorError::Trap {
                        code,
                        pc: self.pc,
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
