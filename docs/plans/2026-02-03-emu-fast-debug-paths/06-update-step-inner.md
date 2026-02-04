# Phase 6: Update step_inner() to Use decode_execute

## Scope of Phase

Update `step_inner()` in `execution.rs` to use the new `decode_execute<M>()` function instead of the old `decode_instruction()` + `execute_instruction()` pattern. This completes the migration to decode-execute fusion.

## Code Organization Reminders

- Place more abstract things, entry points, and tests first
- Place helper utility functions at the bottom of files
- Keep related functionality grouped together

## Implementation Details

### 1. Update execution.rs

Update `lp-riscv/lp-riscv-emu/src/emu/emulator/execution.rs`:

```rust
//! Instruction execution logic.

extern crate alloc;

use super::super::{
    error::EmulatorError,
    executor::{decode_execute, LoggingDisabled, LoggingEnabled},
    memory::Memory,
};
use super::state::Riscv32Emulator;
use super::types::{PanicInfo, StepResult, SyscallInfo};
use alloc::{format, string::String, vec, vec::Vec};
use log;
use lp_riscv_emu_shared::SERIAL_ERROR_INVALID_POINTER;
use lp_riscv_inst::Gpr;

impl Riscv32Emulator {
    /// Execute a single instruction (internal, no fuel check).
    ///
    /// This is the hot path function used by run() loops.
    /// Fuel checking happens in the calling loop, not here.
    #[inline(always)]
    pub(super) fn step_inner(&mut self) -> Result<StepResult, EmulatorError> {
        // Fetch instruction
        let inst_word = self.memory.fetch_instruction(self.pc).map_err(|mut e| {
            match &mut e {
                EmulatorError::InvalidMemoryAccess {
                    regs: err_regs,
                    pc: err_pc,
                    ..
                } => {
                    *err_regs = self.regs;
                    *err_pc = self.pc;
                }
                EmulatorError::UnalignedAccess {
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

        // Check if this is a trap BEFORE executing the instruction
        // For EBREAK instructions, we need to check if the current PC is a trap location
        // Note: We can't check the decoded instruction anymore, so we check opcode directly
        let is_trap_before_execution = if (inst_word & 0x7f) == 0x73 && (inst_word & 0xfffff000) == 0x00100000 {
            // EBREAK: opcode=0x73, funct12=0x001
            self.traps
                .binary_search_by_key(&self.pc, |(addr, _)| *addr)
                .is_ok()
        } else {
            false
        };

        // Execute instruction using decode-execute fusion
        let exec_result = match self.log_level {
            super::super::logging::LogLevel::None => {
                decode_execute::<LoggingDisabled>(
                    inst_word,
                    self.pc,
                    &mut self.regs,
                    &mut self.memory,
                )?
            }
            _ => {
                decode_execute::<LoggingEnabled>(
                    inst_word,
                    self.pc,
                    &mut self.regs,
                    &mut self.memory,
                )?
            }
        };

        // Update PC (2 bytes for compressed, 4 for standard)
        let pc_increment = if is_compressed { 2 } else { 4 };
        self.pc = exec_result
            .new_pc
            .unwrap_or(self.pc.wrapping_add(pc_increment));

        // Log instruction with cycle count (only if logging is enabled)
        if let Some(log) = exec_result.log {
            let log_with_cycle = log.set_cycle(self.instruction_count);
            self.log_instruction(log_with_cycle);
        }

        // Handle special cases
        if exec_result.should_halt {
            if is_trap_before_execution {
                // This was a trap - find the trap code using the original PC (before PC update)
                let original_pc = self.pc.saturating_sub(pc_increment);
                let index = self
                    .traps
                    .binary_search_by_key(&original_pc, |(addr, _)| *addr)
                    .expect("Trap should be found since is_trap_before_execution was true");
                let trap_code = self.traps[index].1;
                Ok(StepResult::Trap(trap_code))
            } else {
                // Regular ebreak (not a trap)
                Ok(StepResult::Halted)
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

            // Handle syscalls (panic, write, log, serial, time, yield)
            // ... keep existing syscall handling logic ...
            
            // (Keep all the existing syscall handling code from the original step_inner)
        } else {
            Ok(StepResult::Continue)
        }
    }

    // ... rest of the file unchanged ...
}
```

**Note**: Keep all the existing syscall handling logic (panic, write, log, serial, time, yield) - just update the instruction execution part.

### 2. Remove decode_instruction Import

Remove the import of `decode_instruction` since we're no longer using it:

```rust
// Remove this:
// use super::super::decoder::decode_instruction;
```

### 3. Update Trap Detection

Since we no longer have the decoded `Inst` enum, we need to detect EBREAK by checking the instruction word directly:

```rust
// EBREAK: opcode=0x73, funct12=0x001
let is_ebreak = (inst_word & 0x7f) == 0x73 && (inst_word & 0xfffff000) == 0x00100000;
```

## Tests

Run all tests to ensure nothing breaks:

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
- `step_inner()` uses `decode_execute<M>()`
- No more `decode_instruction()` calls in hot path
- All tests pass
- Syscall handling still works correctly
