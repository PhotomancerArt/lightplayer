# Phase 4: Update run_loops.rs to Use New Dispatch

## Scope of Phase

Update `run_loops.rs` to dispatch to fast or logging paths based on `log_level`, and wire up the new `decode_execute<M>()` function. This connects the new executor to the emulator run loops.

## Code Organization Reminders

- Place more abstract things, entry points, and tests first
- Place helper utility functions at the bottom of files
- Keep related functionality grouped together

## Implementation Details

### 1. Update run_loops.rs

Update `lp-riscv/lp-riscv-emu/src/emu/emulator/run_loops.rs`:

```rust
//! High-level run loop methods.

extern crate alloc;

use super::super::{
    error::EmulatorError,
    executor::{decode_execute, LoggingDisabled, LoggingEnabled},
    logging::LogLevel,
};
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
    /// This dispatches to fast or logging path based on log_level.
    pub(super) fn run_inner(&mut self, mut fuel: u64) -> Result<StepResult, EmulatorError> {
        match self.log_level {
            LogLevel::None => self.run_inner_fast(fuel),
            _ => self.run_inner_logging(fuel),
        }
    }
    
    /// Fast path run loop - zero logging overhead.
    fn run_inner_fast(&mut self, mut fuel: u64) -> Result<StepResult, EmulatorError> {
        let initial_instruction_count = self.instruction_count;

        loop {
            // Inline fuel check - decrement and check in the loop
            fuel -= 1;
            if fuel == 0 {
                let instructions_executed = self.instruction_count - initial_instruction_count;
                return Ok(StepResult::FuelExhausted(instructions_executed));
            }

            // Fetch instruction
            let inst_word = self.memory.fetch_instruction(self.pc).map_err(|mut e| {
                match &mut e {
                    super::super::error::EmulatorError::InvalidMemoryAccess {
                        regs: err_regs,
                        pc: err_pc,
                        ..
                    } => {
                        *err_regs = self.regs;
                        *err_pc = self.pc;
                    }
                    super::super::error::EmulatorError::UnalignedAccess {
                        regs: err_regs,
                        pc: err_pc,
                        ..
                    } => {
                        *err_regs = self.regs;
                        *err_pc = self.pc;
                    }
                    _ => {}
                }
                e
            })?;

            // Check if compressed instruction (bits [1:0] != 0b11)
            let is_compressed = (inst_word & 0x3) != 0x3;

            // Increment instruction count before execution (for cycle counting)
            self.instruction_count += 1;

            // Execute using fast path (no logging)
            let exec_result = decode_execute::<LoggingDisabled>(
                inst_word,
                self.pc,
                &mut self.regs,
                &mut self.memory,
            )?;

            // Update PC (2 bytes for compressed, 4 for standard)
            let pc_increment = if is_compressed { 2 } else { 4 };
            self.pc = exec_result
                .new_pc
                .unwrap_or(self.pc.wrapping_add(pc_increment));

            // Handle results (no logging)
            if exec_result.should_halt {
                // Check if this is a trap BEFORE executing the instruction
                // For EBREAK instructions, we need to check if the current PC is a trap location
                let is_trap = self
                    .traps
                    .binary_search_by_key(&self.pc.saturating_sub(pc_increment), |(addr, _)| *addr)
                    .is_ok();
                
                if is_trap {
                    let original_pc = self.pc.saturating_sub(pc_increment);
                    let index = self
                        .traps
                        .binary_search_by_key(&original_pc, |(addr, _)| *addr)
                        .expect("Trap should be found");
                    let trap_code = self.traps[index].1;
                    return Ok(StepResult::Trap(trap_code));
                } else {
                    return Ok(StepResult::Halted);
                }
            } else if exec_result.syscall {
                // Extract syscall info from registers
                let syscall_info = SyscallInfo {
                    number: self.regs[Gpr::A7.num() as usize],
                    args: [
                        self.regs[Gpr::A0.num() as usize],
                        self.regs[Gpr::A1.num() as usize],
                        self.regs[Gpr::A2.num() as usize],
                        self.regs[Gpr::A3.num() as usize],
                        self.regs[Gpr::A4.num() as usize],
                        self.regs[Gpr::A5.num() as usize],
                        self.regs[Gpr::A6.num() as usize],
                    ],
                };

                // Check if this is a yield syscall
                if syscall_info.number == SYSCALL_YIELD {
                    return Ok(StepResult::Syscall(syscall_info));
                }
                // Other syscalls handled in step_inner() - continue
                continue;
            } else {
                // Most common case - continue execution
                continue;
            }
        }
    }
    
    /// Logging path run loop - full logging support.
    fn run_inner_logging(&mut self, mut fuel: u64) -> Result<StepResult, EmulatorError> {
        let initial_instruction_count = self.instruction_count;

        loop {
            // Inline fuel check - decrement and check in the loop
            fuel -= 1;
            if fuel == 0 {
                let instructions_executed = self.instruction_count - initial_instruction_count;
                return Ok(StepResult::FuelExhausted(instructions_executed));
            }

            // Fetch instruction
            let inst_word = self.memory.fetch_instruction(self.pc).map_err(|mut e| {
                match &mut e {
                    super::super::error::EmulatorError::InvalidMemoryAccess {
                        regs: err_regs,
                        pc: err_pc,
                        ..
                    } => {
                        *err_regs = self.regs;
                        *err_pc = self.pc;
                    }
                    super::super::error::EmulatorError::UnalignedAccess {
                        regs: err_regs,
                        pc: err_pc,
                        ..
                    } => {
                        *err_regs = self.regs;
                        *err_pc = self.pc;
                    }
                    _ => {}
                }
                e
            })?;

            // Check if compressed instruction (bits [1:0] != 0b11)
            let is_compressed = (inst_word & 0x3) != 0x3;

            // Increment instruction count before execution (for cycle counting)
            self.instruction_count += 1;

            // Execute using logging path
            let exec_result = decode_execute::<LoggingEnabled>(
                inst_word,
                self.pc,
                &mut self.regs,
                &mut self.memory,
            )?;

            // Update PC (2 bytes for compressed, 4 for standard)
            let pc_increment = if is_compressed { 2 } else { 4 };
            self.pc = exec_result
                .new_pc
                .unwrap_or(self.pc.wrapping_add(pc_increment));

            // Handle logging
            if let Some(log) = exec_result.log {
                let log_with_cycle = log.set_cycle(self.instruction_count);
                self.log_instruction(log_with_cycle);
            }

            // Handle results (same as fast path)
            if exec_result.should_halt {
                let is_trap = self
                    .traps
                    .binary_search_by_key(&self.pc.saturating_sub(pc_increment), |(addr, _)| *addr)
                    .is_ok();
                
                if is_trap {
                    let original_pc = self.pc.saturating_sub(pc_increment);
                    let index = self
                        .traps
                        .binary_search_by_key(&original_pc, |(addr, _)| *addr)
                        .expect("Trap should be found");
                    let trap_code = self.traps[index].1;
                    return Ok(StepResult::Trap(trap_code));
                } else {
                    return Ok(StepResult::Halted);
                }
            } else if exec_result.syscall {
                let syscall_info = SyscallInfo {
                    number: self.regs[Gpr::A7.num() as usize],
                    args: [
                        self.regs[Gpr::A0.num() as usize],
                        self.regs[Gpr::A1.num() as usize],
                        self.regs[Gpr::A2.num() as usize],
                        self.regs[Gpr::A3.num() as usize],
                        self.regs[Gpr::A4.num() as usize],
                        self.regs[Gpr::A5.num() as usize],
                        self.regs[Gpr::A6.num() as usize],
                    ],
                };

                if syscall_info.number == SYSCALL_YIELD {
                    return Ok(StepResult::Syscall(syscall_info));
                }
                continue;
            } else {
                continue;
            }
        }
    }

    // ... keep existing run(), run_fuel(), etc. methods unchanged ...
}
```

**Note**: The syscall handling logic (panic, write, log, serial, etc.) should remain in `execution.rs`'s `step_inner()` for now. We'll migrate that in phase 6.

### 2. Update execution.rs Temporarily

For now, `step_inner()` should still use the old path until phase 6. We can add a TODO comment:

```rust
// TODO: In phase 6, update step_inner() to use decode_execute<M>()
```

## Tests

Run existing tests to ensure nothing breaks:

```bash
cd lp-riscv/lp-riscv-emu
cargo test
```

## Validate

Run:
```bash
cd lp-riscv/lp-riscv-emu
cargo check
cargo test
```

Ensure:
- `run_inner()` dispatches correctly based on `log_level`
- Fast path works (test with `LogLevel::None`)
- Logging path works (test with `LogLevel::Instructions`)
- All existing tests still pass
