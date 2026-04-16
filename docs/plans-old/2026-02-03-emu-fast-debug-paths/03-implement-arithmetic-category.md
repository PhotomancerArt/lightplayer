# Phase 3: Implement Arithmetic Category as Proof of Concept

## Scope of Phase

Implement the arithmetic category (`arithmetic.rs`) with R-type instructions as a proof of concept. This validates the monomorphic approach works correctly before implementing all categories.

## Code Organization Reminders

- Place more abstract things, entry points, and tests first
- Place helper utility functions at the bottom of files
- Keep related functionality grouped together
- Each instruction gets its own `#[inline]` function

## Implementation Details

### 1. Create arithmetic.rs

Create `lp-riscv/lp-riscv-emu/src/emu/executor/arithmetic.rs`:

```rust
//! Arithmetic instruction execution (R-type: ADD, SUB, MUL, etc.)

use super::super::super::{error::EmulatorError, logging::InstLog, memory::Memory};
use super::{read_reg, ExecutionResult, LoggingMode};
use lp_riscv_inst::{format::TypeR, Gpr};

/// Decode and execute R-type arithmetic instructions.
pub(super) fn decode_execute_rtype<M: LoggingMode>(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    _memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError>
where
    [(); M::ENABLED as usize]:,
{
    let r = TypeR::from_riscv(inst_word);
    let rd = Gpr::new(r.rd);
    let rs1 = Gpr::new(r.rs1);
    let rs2 = Gpr::new(r.rs2);
    let funct3 = (r.func & 0x7) as u8;
    let funct7 = ((r.func >> 3) & 0x7f) as u8;
    
    match (funct3, funct7) {
        (0x0, 0x0) => execute_add::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x0, 0x20) => execute_sub::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x0, 0x01) => execute_mul::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x1, 0x01) => execute_mulh::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x2, 0x01) => execute_mulhsu::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x3, 0x01) => execute_mulhu::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x4, 0x01) => execute_div::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x5, 0x01) => execute_divu::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x6, 0x01) => execute_rem::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x7, 0x01) => execute_remu::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x2, 0x0) => execute_slt::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x3, 0x0) => execute_sltu::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x4, 0x0) => execute_xor::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x6, 0x0) => execute_or::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x7, 0x0) => execute_and::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x1, 0x0) => execute_sll::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x5, 0x0) => execute_srl::<M>(rd, rs1, rs2, inst_word, pc, regs),
        (0x5, 0x20) => execute_sra::<M>(rd, rs1, rs2, inst_word, pc, regs),
        _ => Err(EmulatorError::InvalidInstruction {
            pc,
            instruction: inst_word,
            reason: alloc::format!(
                "Unknown R-type instruction: funct3=0x{:x}, funct7=0x{:x}",
                funct3, funct7
            ),
            regs: *regs,
        }),
    }
}

#[inline(always)]
fn execute_add<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError>
where
    [(); M::ENABLED as usize]:,
{
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED {
        read_reg(regs, rd)
    } else {
        0
    };
    let result = val1.wrapping_add(val2);
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
        Some(InstLog::Arithmetic {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rd,
            rs1_val: val1,
            rs2_val: Some(val2),
            rd_old,
            rd_new: result,
        })
    } else {
        None
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

#[inline(always)]
fn execute_sub<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError>
where
    [(); M::ENABLED as usize]:,
{
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED {
        read_reg(regs, rd)
    } else {
        0
    };
    let result = val1.wrapping_sub(val2);
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
        Some(InstLog::Arithmetic {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rd,
            rs1_val: val1,
            rs2_val: Some(val2),
            rd_old,
            rd_new: result,
        })
    } else {
        None
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

// TODO: Implement remaining arithmetic instructions (MUL, DIV, etc.)
// For now, implement at least execute_mul to test the pattern

#[inline(always)]
fn execute_mul<M: LoggingMode>(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError>
where
    [(); M::ENABLED as usize]:,
{
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = if M::ENABLED {
        read_reg(regs, rd)
    } else {
        0
    };
    let result = val1.wrapping_mul(val2);
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if M::ENABLED {
        Some(InstLog::Arithmetic {
            cycle: 0,
            pc,
            instruction: instruction_word,
            rd,
            rs1_val: val1,
            rs2_val: Some(val2),
            rd_old,
            rd_new: result,
        })
    } else {
        None
    };
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log,
    })
}

// Add remaining instructions following the same pattern...
// (execute_mulh, execute_mulhsu, execute_mulhu, execute_div, etc.)
```

**Note**: For this phase, implement at least `execute_add`, `execute_sub`, and `execute_mul` to validate the pattern. The remaining instructions can be implemented in phase 5.

### 2. Update executor/mod.rs

Uncomment the arithmetic module and wire it up:

```rust
pub mod arithmetic;

// ... in decode_execute function, update the 0x33 case:
0x33 => arithmetic::decode_execute_rtype::<M>(inst_word, pc, regs, memory),
```

### 3. Add extern crate alloc

Make sure `arithmetic.rs` has:

```rust
extern crate alloc;
```

## Tests

Create a simple test to verify the monomorphic approach works:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::emu::{memory::Memory, LoggingDisabled, LoggingEnabled};
    
    #[test]
    fn test_add_fast_path() {
        let mut regs = [0i32; 32];
        regs[1] = 10;
        regs[2] = 20;
        let mut memory = Memory::with_default_addresses(vec![], vec![]);
        
        // Test ADD instruction: add x3, x1, x2
        let inst_word = 0x002081b3; // ADD x3, x1, x2
        let result = decode_execute::<LoggingDisabled>(inst_word, 0, &mut regs, &mut memory).unwrap();
        
        assert_eq!(regs[3], 30);
        assert!(result.log.is_none());
    }
    
    #[test]
    fn test_add_logging_path() {
        let mut regs = [0i32; 32];
        regs[1] = 10;
        regs[2] = 20;
        let mut memory = Memory::with_default_addresses(vec![], vec![]);
        
        // Test ADD instruction: add x3, x1, x2
        let inst_word = 0x002081b3; // ADD x3, x1, x2
        let result = decode_execute::<LoggingEnabled>(inst_word, 0, &mut regs, &mut memory).unwrap();
        
        assert_eq!(regs[3], 30);
        assert!(result.log.is_some());
        if let Some(InstLog::Arithmetic { rd_new, .. }) = result.log {
            assert_eq!(rd_new, 30);
        }
    }
}
```

## Validate

Run:
```bash
cd lp-riscv/lp-riscv-emu
cargo test executor::arithmetic
cargo check
```

Ensure:
- Arithmetic instructions compile and work
- Fast path has no logging overhead
- Logging path creates InstLog entries
- Tests pass
