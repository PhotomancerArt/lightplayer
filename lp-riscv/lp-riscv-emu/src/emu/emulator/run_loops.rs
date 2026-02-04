//! High-level run loop methods.

extern crate alloc;

use super::super::error::EmulatorError;
use super::state::Riscv32Emulator;
use super::types::{StepResult, SyscallInfo};
use alloc::string::String;
use lp_riscv_emu_shared::SYSCALL_YIELD;
use lp_riscv_inst::Gpr;

/// Default fuel for run() function
const DEFAULT_FUEL: u64 = 100_000;

impl Riscv32Emulator {
    /// Internal run loop with tight loop and inline fuel checking.
    ///
    /// This is the hot path - fuel checking happens inline in the loop
    /// to minimize function call overhead.
    ///
    /// # Arguments
    /// * `fuel` - Maximum number of instructions to execute before returning FuelExhausted
    ///
    /// # Returns
    /// * `Ok(StepResult::Syscall(info))` - Yield syscall encountered (SYSCALL_YIELD)
    /// * `Ok(StepResult::Halted)` - EBREAK encountered (not a trap)
    /// * `Ok(StepResult::Trap(code))` - Trap encountered
    /// * `Ok(StepResult::Panic(info))` - Panic occurred
    /// * `Ok(StepResult::FuelExhausted(count))` - Fuel exhausted (instructions executed)
    /// * `Err(EmulatorError)` - Error occurred (memory access violation, etc.)
    pub(super) fn run_inner(&mut self, mut fuel: u64) -> Result<StepResult, EmulatorError> {
        let initial_instruction_count = self.instruction_count;

        loop {
            // Inline fuel check - decrement and check in the loop
            fuel -= 1;
            if fuel == 0 {
                let instructions_executed = self.instruction_count - initial_instruction_count;
                return Ok(StepResult::FuelExhausted(instructions_executed));
            }

            // Call step_inner() (no fuel check, already checked above)
            match self.step_inner()? {
                StepResult::Continue => {
                    // Most common case - continue execution
                    continue;
                }
                StepResult::Syscall(info) => {
                    // Check if this is a yield syscall
                    if info.number == SYSCALL_YIELD {
                        return Ok(StepResult::Syscall(info));
                    }
                    // Other syscall - continue (handled internally by step_inner)
                    continue;
                }
                StepResult::Halted => {
                    return Ok(StepResult::Halted);
                }
                StepResult::Trap(code) => {
                    return Ok(StepResult::Trap(code));
                }
                StepResult::Panic(info) => {
                    return Ok(StepResult::Panic(info));
                }
                StepResult::FuelExhausted(_) => {
                    // step_inner() should never return FuelExhausted
                    unreachable!("step_inner() should never return FuelExhausted");
                }
            }
        }
    }

    /// Run the emulator with default fuel until yield, halt, trap, panic, or fuel exhaustion.
    ///
    /// Uses default fuel (100_000 instructions). For custom fuel, use `run_fuel()`.
    ///
    /// # Returns
    /// * `Ok(StepResult::Syscall(info))` - Yield syscall encountered (SYSCALL_YIELD)
    /// * `Ok(StepResult::Halted)` - EBREAK encountered (not a trap)
    /// * `Ok(StepResult::Trap(code))` - Trap encountered
    /// * `Ok(StepResult::Panic(info))` - Panic occurred
    /// * `Ok(StepResult::FuelExhausted(count))` - Fuel exhausted (instructions executed)
    /// * `Err(EmulatorError)` - Error occurred (memory access violation, etc.)
    pub fn run(&mut self) -> Result<StepResult, EmulatorError> {
        self.run_fuel(DEFAULT_FUEL)
    }

    /// Run the emulator with specified fuel until yield, halt, trap, panic, or fuel exhaustion.
    ///
    /// # Arguments
    /// * `fuel` - Maximum number of instructions to execute before returning FuelExhausted
    ///
    /// # Returns
    /// * `Ok(StepResult::Syscall(info))` - Yield syscall encountered (SYSCALL_YIELD)
    /// * `Ok(StepResult::Halted)` - EBREAK encountered (not a trap)
    /// * `Ok(StepResult::Trap(code))` - Trap encountered
    /// * `Ok(StepResult::Panic(info))` - Panic occurred
    /// * `Ok(StepResult::FuelExhausted(count))` - Fuel exhausted (instructions executed)
    /// * `Err(EmulatorError)` - Error occurred (memory access violation, etc.)
    pub fn run_fuel(&mut self, fuel: u64) -> Result<StepResult, EmulatorError> {
        self.run_inner(fuel)
    }

    /// Run until EBREAK is encountered, returning the value in a0.
    pub fn run_until_ebreak(&mut self) -> Result<i32, EmulatorError> {
        loop {
            match self.run()? {
                StepResult::Halted => {
                    return Ok(self.regs[Gpr::A0.num() as usize]);
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
                StepResult::FuelExhausted(_) => {
                    // Continue running - use more fuel
                    continue;
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
                StepResult::Continue => {
                    // run() should not return Continue
                    unreachable!("run() should not return Continue");
                }
            }
        }
    }

    /// Run until ECALL is encountered, returning syscall information.
    pub fn run_until_ecall(&mut self) -> Result<SyscallInfo, EmulatorError> {
        loop {
            match self.run()? {
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
                StepResult::FuelExhausted(_) => {
                    // Continue running - use more fuel
                    continue;
                }
                StepResult::Continue => {
                    // run() should not return Continue
                    unreachable!("run() should not return Continue");
                }
            }
        }
    }

    /// Run until a yield syscall is encountered, with a maximum step limit
    ///
    /// Steps the emulator until a yield syscall (SYSCALL_YIELD) is encountered,
    /// or until the maximum number of steps is reached.
    ///
    /// # Arguments
    /// * `max_steps` - Maximum number of steps to execute
    ///
    /// # Returns
    /// * `Ok(SyscallInfo)` - Yield syscall was encountered
    /// * `Err(EmulatorError)` - Error occurred (trap, panic, or max steps exceeded)
    pub fn run_until_yield(&mut self, max_steps: u64) -> Result<SyscallInfo, EmulatorError> {
        loop {
            match self.run_fuel(max_steps)? {
                StepResult::Syscall(info) if info.number == SYSCALL_YIELD => {
                    return Ok(info);
                }
                StepResult::Syscall(_) => {
                    // Other syscall - continue execution (but run() only returns yield syscalls)
                    // This shouldn't happen, but handle it gracefully
                    continue;
                }
                StepResult::Halted => {
                    return Err(EmulatorError::InvalidInstruction {
                        pc: self.pc,
                        instruction: 0,
                        reason: String::from("Unexpected EBREAK in run_until_yield"),
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
                StepResult::FuelExhausted(_) => {
                    // Fuel exhausted - this means we hit max_steps
                    return Err(EmulatorError::InstructionLimitExceeded {
                        limit: max_steps,
                        executed: max_steps,
                        pc: self.pc,
                        regs: self.regs,
                    });
                }
                StepResult::Continue => {
                    // run() should not return Continue
                    unreachable!("run() should not return Continue");
                }
            }
        }
    }
}
