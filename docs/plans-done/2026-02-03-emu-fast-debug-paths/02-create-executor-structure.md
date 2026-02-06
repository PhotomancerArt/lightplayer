# Phase 2: Create Executor Directory Structure and Main Dispatch

## Scope of Phase

Create the executor directory structure and implement the main `decode_execute<M>()` dispatch function. This sets up the routing infrastructure before implementing individual categories.

## Code Organization Reminders

- Place more abstract things, entry points, and tests first
- Place helper utility functions at the bottom of files
- Keep related functionality grouped together

## Implementation Details

### 1. Create ExecutionResult in executor/mod.rs

Update `lp-riscv/lp-riscv-emu/src/emu/executor/mod.rs`:

```rust
//! Instruction executor for RISC-V 32-bit instructions.

use super::super::{error::EmulatorError, logging::InstLog, memory::Memory};

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
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError>
where
    [(); M::ENABLED as usize]:,
{
    // Check if compressed instruction (bits [1:0] != 0b11)
    if (inst_word & 0x3) != 0x3 {
        // TODO: Implement compressed instructions in phase 5
        return Err(EmulatorError::InvalidInstruction {
            pc,
            instruction: inst_word,
            reason: alloc::string::String::from("Compressed instructions not yet implemented"),
            regs: *regs,
        });
    }
    
    let opcode = (inst_word & 0x7f) as u8;
    
    match opcode {
        0x33 => {
            // R-type (arithmetic)
            // TODO: Implement in phase 3
            Err(EmulatorError::InvalidInstruction {
                pc,
                instruction: inst_word,
                reason: alloc::string::String::from("R-type instructions not yet implemented"),
                regs: *regs,
            })
        }
        0x13 => {
            // I-type (immediate arithmetic/logical/shift)
            // TODO: Implement in phase 5
            Err(EmulatorError::InvalidInstruction {
                pc,
                instruction: inst_word,
                reason: alloc::string::String::from("I-type instructions not yet implemented"),
                regs: *regs,
            })
        }
        0x03 => {
            // Load instructions
            // TODO: Implement in phase 5
            Err(EmulatorError::InvalidInstruction {
                pc,
                instruction: inst_word,
                reason: alloc::string::String::from("Load instructions not yet implemented"),
                regs: *regs,
            })
        }
        0x23 => {
            // Store instructions
            // TODO: Implement in phase 5
            Err(EmulatorError::InvalidInstruction {
                pc,
                instruction: inst_word,
                reason: alloc::string::String::from("Store instructions not yet implemented"),
                regs: *regs,
            })
        }
        0x63 => {
            // Branch instructions
            // TODO: Implement in phase 5
            Err(EmulatorError::InvalidInstruction {
                pc,
                instruction: inst_word,
                reason: alloc::string::String::from("Branch instructions not yet implemented"),
                regs: *regs,
            })
        }
        0x6f => {
            // JAL
            // TODO: Implement in phase 5
            Err(EmulatorError::InvalidInstruction {
                pc,
                instruction: inst_word,
                reason: alloc::string::String::from("JAL not yet implemented"),
                regs: *regs,
            })
        }
        0x67 => {
            // JALR
            // TODO: Implement in phase 5
            Err(EmulatorError::InvalidInstruction {
                pc,
                instruction: inst_word,
                reason: alloc::string::String::from("JALR not yet implemented"),
                regs: *regs,
            })
        }
        0x37 => {
            // LUI
            // TODO: Implement in phase 5
            Err(EmulatorError::InvalidInstruction {
                pc,
                instruction: inst_word,
                reason: alloc::string::String::from("LUI not yet implemented"),
                regs: *regs,
            })
        }
        0x17 => {
            // AUIPC
            // TODO: Implement in phase 5
            Err(EmulatorError::InvalidInstruction {
                pc,
                instruction: inst_word,
                reason: alloc::string::String::from("AUIPC not yet implemented"),
                regs: *regs,
            })
        }
        0x73 => {
            // System instructions (ECALL, EBREAK, CSR)
            // TODO: Implement in phase 5
            Err(EmulatorError::InvalidInstruction {
                pc,
                instruction: inst_word,
                reason: alloc::string::String::from("System instructions not yet implemented"),
                regs: *regs,
            })
        }
        _ => Err(EmulatorError::InvalidInstruction {
            pc,
            instruction: inst_word,
            reason: alloc::format!("Unknown opcode: 0x{:02x}", opcode),
            regs: *regs,
        }),
    }
}
```

### 2. Add extern crate alloc

Make sure `executor/mod.rs` has:

```rust
extern crate alloc;
```

### 3. Create Placeholder Category Files

Create empty category files to establish the structure:

- `lp-riscv/lp-riscv-emu/src/emu/executor/arithmetic.rs` (empty for now)
- `lp-riscv/lp-riscv-emu/src/emu/executor/immediate.rs` (empty for now)
- `lp-riscv/lp-riscv-emu/src/emu/executor/load_store.rs` (empty for now)
- `lp-riscv/lp-riscv-emu/src/emu/executor/branch.rs` (empty for now)
- `lp-riscv/lp-riscv-emu/src/emu/executor/jump.rs` (empty for now)
- `lp-riscv/lp-riscv-emu/src/emu/executor/system.rs` (empty for now)
- `lp-riscv/lp-riscv-emu/src/emu/executor/compressed.rs` (empty for now)
- `lp-riscv/lp-riscv-emu/src/emu/executor/bitmanip.rs` (empty for now)

Each file should have:

```rust
//! [Category name] instruction execution.
// TODO: Implement in phase 5
```

### 4. Update mod.rs to Include Modules

Update `lp-riscv/lp-riscv-emu/src/emu/executor/mod.rs` to include modules (commented out for now):

```rust
// Category modules - will be uncommented as they're implemented
// pub mod arithmetic;
// pub mod immediate;
// pub mod load_store;
// pub mod branch;
// pub mod jump;
// pub mod system;
// pub mod compressed;
// pub mod bitmanip;
```

## Tests

No new tests needed - this is just infrastructure setup.

## Validate

Run:
```bash
cd lp-riscv/lp-riscv-emu
cargo check
```

Ensure:
- `decode_execute<M>()` function compiles
- All placeholder error messages are clear
- Module structure is correct
