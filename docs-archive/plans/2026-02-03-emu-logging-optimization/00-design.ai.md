# Emulator Logging and Decode-Execute Optimization - Design

## Scope of Work

Improve emulator performance by:
1. **Eliminate logging overhead when disabled** - Use dual implementations with runtime dispatch to remove logging overhead when `LogLevel::None`
2. **Optimize instruction decoding** - Use lookup tables and decode-execute fusion for common instructions
3. **Reorganize instruction execution** - Split instructions into separate files for maintainability and future extensibility (floating point, etc.)

**Expected Impact**: 15-25% improvement from removing logging overhead, 10-15% improvement from optimized decoding

## Current Problems

### 1. Logging Overhead (High Impact)

**Current Issues**:
- Even with `LogLevel::None`, the executor:
  - Checks `log_level != LogLevel::None` for every instruction (runtime check)
  - Reads `rd_old` values unnecessarily (even when logging disabled)
  - Creates `Option<InstLog>` structures (allocations even when None)
  - Pattern matching on log level in hot path

**Example from current code**:
```rust
let rd_old = if log_level != LogLevel::None {
    Some(read_reg(regs, rd))
} else {
    None
};
// ... execute instruction ...
if log_level != LogLevel::None {
    Some(InstLog::Arithmetic { ... })
} else {
    None
}
```

**Impact**: 15-25% performance penalty when logging is disabled

### 2. Decode-Execute Separation (Medium Impact)

**Current Issues**:
- `decode_instruction()` does extensive pattern matching and string formatting for errors
- Separate decode and execute steps add overhead
- String formatting in error paths (even if not executed, adds code size)
- No lookup tables for common opcodes

**Impact**: 10-15% improvement potential

### 3. Code Organization (Maintainability)

**Current Issues**:
- All instructions in single `executor.rs` file (~3500+ lines)
- Hard to extend with floating point, vector extensions, etc.
- Difficult to navigate and maintain

## Proposed Solution

### 1. Dual Implementation Approach (Selected)

**Requirement**: We need runtime log control (filetests set `LogLevel::Instructions` for detail mode, `LogLevel::None` for speed), but zero overhead when logging is disabled.

**Approach**: Two complete implementations side-by-side
- **Fast path**: `execute_instruction_fast()` - zero logging overhead, no checks, no register reads for logging
- **Logging path**: `execute_instruction_logging()` - full logging support with runtime verbosity control
- **Dispatch**: Two `run_inner()` functions that dispatch to the appropriate instruction implementations
- Functions live next to each other for clarity and maintainability

**Structure**:

```rust
// In executor.rs or executor/mod.rs

// Fast path: zero logging overhead
// Delegates to category-specific functions
pub fn execute_instruction_fast(
    inst: Inst,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    // Route to appropriate category function
    match inst {
        // Arithmetic instructions
        Inst::Add { .. } | Inst::Sub { .. } | Inst::Mul { .. } | Inst::Mulh { .. }
        | Inst::Mulhsu { .. } | Inst::Mulhu { .. } | Inst::Div { .. } | Inst::Divu { .. }
        | Inst::Rem { .. } | Inst::Remu { .. } | Inst::Slt { .. } | Inst::Sltu { .. }
        | Inst::Xor { .. } | Inst::Or { .. } | Inst::And { .. } | Inst::Sll { .. }
        | Inst::Srl { .. } | Inst::Sra { .. } => {
            arithmetic::execute_arithmetic_fast(inst, instruction_word, pc, regs, memory)
        }
        // Immediate instructions
        Inst::Addi { .. } | Inst::Slti { .. } | Inst::Sltiu { .. } | Inst::Xori { .. }
        | Inst::Ori { .. } | Inst::Andi { .. } | Inst::Slli { .. } | Inst::Srli { .. }
        | Inst::Srai { .. } => {
            immediate::execute_immediate_fast(inst, instruction_word, pc, regs, memory)
        }
        // Load/store instructions
        Inst::Lb { .. } | Inst::Lh { .. } | Inst::Lw { .. } | Inst::Lbu { .. }
        | Inst::Lhu { .. } | Inst::Sb { .. } | Inst::Sh { .. } | Inst::Sw { .. } => {
            load_store::execute_load_store_fast(inst, instruction_word, pc, regs, memory)
        }
        // Branch instructions
        Inst::Beq { .. } | Inst::Bne { .. } | Inst::Blt { .. } | Inst::Bge { .. }
        | Inst::Bltu { .. } | Inst::Bgeu { .. } => {
            branch::execute_branch_fast(inst, instruction_word, pc, regs, memory)
        }
        // Jump instructions
        Inst::Jal { .. } | Inst::Jalr { .. } => {
            jump::execute_jump_fast(inst, instruction_word, pc, regs, memory)
        }
        // System instructions
        Inst::Ecall | Inst::Ebreak | Inst::Csrrw { .. } | Inst::Csrrs { .. }
        | Inst::Csrrc { .. } | Inst::Csrrwi { .. } | Inst::Csrrsi { .. } | Inst::Csrrci { .. } => {
            system::execute_system_fast(inst, instruction_word, pc, regs, memory)
        }
        // ... other instruction categories
    }
}

// Logging path: full runtime control
// Delegates to category-specific functions
pub fn execute_instruction_logging(
    inst: Inst,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
    log_level: LogLevel,
) -> Result<ExecutionResult, EmulatorError> {
    // Route to appropriate category function
    match inst {
        // Arithmetic instructions
        Inst::Add { .. } | Inst::Sub { .. } | Inst::Mul { .. } | Inst::Mulh { .. }
        | Inst::Mulhsu { .. } | Inst::Mulhu { .. } | Inst::Div { .. } | Inst::Divu { .. }
        | Inst::Rem { .. } | Inst::Remu { .. } | Inst::Slt { .. } | Inst::Sltu { .. }
        | Inst::Xor { .. } | Inst::Or { .. } | Inst::And { .. } | Inst::Sll { .. }
        | Inst::Srl { .. } | Inst::Sra { .. } => {
            arithmetic::execute_arithmetic_logging(inst, instruction_word, pc, regs, memory, log_level)
        }
        // Immediate instructions
        Inst::Addi { .. } | Inst::Slti { .. } | Inst::Sltiu { .. } | Inst::Xori { .. }
        | Inst::Ori { .. } | Inst::Andi { .. } | Inst::Slli { .. } | Inst::Srli { .. }
        | Inst::Srai { .. } => {
            immediate::execute_immediate_logging(inst, instruction_word, pc, regs, memory, log_level)
        }
        // Load/store instructions
        Inst::Lb { .. } | Inst::Lh { .. } | Inst::Lw { .. } | Inst::Lbu { .. }
        | Inst::Lhu { .. } | Inst::Sb { .. } | Inst::Sh { .. } | Inst::Sw { .. } => {
            load_store::execute_load_store_logging(inst, instruction_word, pc, regs, memory, log_level)
        }
        // Branch instructions
        Inst::Beq { .. } | Inst::Bne { .. } | Inst::Blt { .. } | Inst::Bge { .. }
        | Inst::Bltu { .. } | Inst::Bgeu { .. } => {
            branch::execute_branch_logging(inst, instruction_word, pc, regs, memory, log_level)
        }
        // Jump instructions
        Inst::Jal { .. } | Inst::Jalr { .. } => {
            jump::execute_jump_logging(inst, instruction_word, pc, regs, memory, log_level)
        }
        // System instructions
        Inst::Ecall | Inst::Ebreak | Inst::Csrrw { .. } | Inst::Csrrs { .. }
        | Inst::Csrrc { .. } | Inst::Csrrwi { .. } | Inst::Csrrsi { .. } | Inst::Csrrci { .. } => {
            system::execute_system_logging(inst, instruction_word, pc, regs, memory, log_level)
        }
        // ... other instruction categories
    }
}

// Public API dispatches based on log_level
pub fn execute_instruction(
    inst: Inst,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
    log_level: LogLevel,
) -> Result<ExecutionResult, EmulatorError> {
    match log_level {
        LogLevel::None => execute_instruction_fast(inst, instruction_word, pc, regs, memory),
        _ => execute_instruction_logging(inst, instruction_word, pc, regs, memory, log_level),
    }
}
```

**In run_loops.rs**:

```rust
impl Riscv32Emulator {
    // Fast path run loop
    pub(super) fn run_inner_fast(&mut self, mut fuel: u64) -> Result<StepResult, EmulatorError> {
        let initial_instruction_count = self.instruction_count;

        loop {
            fuel -= 1;
            if fuel == 0 {
                let instructions_executed = self.instruction_count - initial_instruction_count;
                return Ok(StepResult::FuelExhausted(instructions_executed));
            }

            // Fetch and decode
            let inst_word = self.memory.fetch_instruction(self.pc)?;
            let is_compressed = (inst_word & 0x3) != 0x3;
            let decoded = decode_instruction(inst_word)?;
            
            self.instruction_count += 1;

            // Execute using fast path
            let exec_result = execute_instruction_fast(
                decoded,
                inst_word,
                self.pc,
                &mut self.regs,
                &mut self.memory,
            )?;

            // Update PC
            let pc_increment = if is_compressed { 2 } else { 4 };
            self.pc = exec_result.new_pc.unwrap_or(self.pc.wrapping_add(pc_increment));

            // Handle results (no logging)
            if exec_result.should_halt {
                return Ok(StepResult::Halted);
            } else if exec_result.syscall {
                // Handle syscall...
                continue;
            } else {
                continue;
            }
        }
    }

    // Logging path run loop
    pub(super) fn run_inner_logging(&mut self, mut fuel: u64) -> Result<StepResult, EmulatorError> {
        let initial_instruction_count = self.instruction_count;

        loop {
            fuel -= 1;
            if fuel == 0 {
                let instructions_executed = self.instruction_count - initial_instruction_count;
                return Ok(StepResult::FuelExhausted(instructions_executed));
            }

            // Fetch and decode
            let inst_word = self.memory.fetch_instruction(self.pc)?;
            let is_compressed = (inst_word & 0x3) != 0x3;
            let decoded = decode_instruction(inst_word)?;
            
            self.instruction_count += 1;

            // Execute using logging path
            let exec_result = execute_instruction_logging(
                decoded,
                inst_word,
                self.pc,
                &mut self.regs,
                &mut self.memory,
                self.log_level,
            )?;

            // Update PC
            let pc_increment = if is_compressed { 2 } else { 4 };
            self.pc = exec_result.new_pc.unwrap_or(self.pc.wrapping_add(pc_increment));

            // Handle logging
            if let Some(log) = exec_result.log {
                let log_with_cycle = log.set_cycle(self.instruction_count);
                self.log_instruction(log_with_cycle);
            }

            // Handle results
            if exec_result.should_halt {
                return Ok(StepResult::Halted);
            } else if exec_result.syscall {
                // Handle syscall...
                continue;
            } else {
                continue;
            }
        }
    }

    // Public API dispatches to appropriate run loop
    pub(super) fn run_inner(&mut self, fuel: u64) -> Result<StepResult, EmulatorError> {
        match self.log_level {
            LogLevel::None => self.run_inner_fast(fuel),
            _ => self.run_inner_logging(fuel),
        }
    }
}
```

**Benefits**:
- **Zero overhead in fast path**: No log_level checks, no register reads for logging, no InstLog allocations
- **Runtime control**: Full logging support when enabled, with runtime verbosity control
- **Clear separation**: Fast and logging implementations are side-by-side, easy to maintain
- **Compiler optimization**: Each path can be optimized independently by the compiler
- **No code duplication in hot path**: The actual instruction execution logic is separate from logging

**Trade-offs**:
- **Code duplication**: Two implementations of each instruction (but this is intentional for performance)
- **Maintenance**: Changes to instruction logic need to be made in both places (but they're next to each other)
- **File size**: Larger files, but this is mitigated by splitting into category files

**Mitigation for code duplication**:
- Keep fast and logging versions next to each other for easy comparison
- Use comments to mark corresponding implementations
- Consider helper functions for shared logic (but avoid in hot path)

### 2. Decode-Execute Fusion

Follow embive's approach: decode directly into execution for common instructions.

**Structure**:
```rust
// Instead of: decode() -> Inst -> execute()
// Do: decode_execute() -> Result<ExecutionResult>

pub fn decode_execute(
    inst_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
    log_level: LogLevel,
) -> Result<ExecutionResult, EmulatorError> {
    // Fast path: check compressed first
    if (inst_word & 0x3) != 0x3 {
        return decode_execute_compressed(inst_word, pc, regs, memory, log_level);
    }
    
    let opcode = (inst_word & 0x7f) as u8;
    
    // Use lookup table for common opcodes
    match opcode {
        0x33 => decode_execute_rtype(inst_word, pc, regs, memory, log_level),
        0x13 => decode_execute_itype(inst_word, pc, regs, memory, log_level),
        0x03 => decode_execute_load(inst_word, pc, regs, memory, log_level),
        0x23 => decode_execute_store(inst_word, pc, regs, memory, log_level),
        // ... etc
    }
}
```

**Benefits**:
- Eliminates intermediate `Inst` enum allocation
- Reduces pattern matching overhead
- Allows instruction-specific optimizations
- Better code locality

### 3. File Organization

Split instructions into separate modules. Each file contains **both** fast and logging implementations side-by-side:

```
lp-riscv/lp-riscv-emu/src/emu/executor/
├── mod.rs                    # Main dispatch: execute_instruction() and decode_execute()
├── arithmetic.rs             # ADD, SUB, MUL, etc. (fast + logging)
├── immediate.rs              # ADDI, SLLI, SRLI, etc. (fast + logging)
├── load_store.rs            # LW, SW, LB, etc. (fast + logging)
├── branch.rs                 # BEQ, BNE, BLT, etc. (fast + logging)
├── jump.rs                   # JAL, JALR (fast + logging)
├── system.rs                 # ECALL, EBREAK, CSR (fast + logging)
├── compressed.rs             # C.ADD, C.LW, etc. (fast + logging)
└── floating_point.rs         # Future: FADD, FMUL, etc. (fast + logging)
```

**File structure example** (`arithmetic.rs`):

```rust
//! Arithmetic instruction execution (ADD, SUB, MUL, etc.)

use super::super::{error::EmulatorError, memory::Memory};
use super::{ExecutionResult, read_reg};
use lp_riscv_inst::{Gpr, Inst};

// ============================================================================
// Fast Path (No Logging)
// ============================================================================

pub(super) fn execute_add_fast(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let result = val1.wrapping_add(val2);
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    Ok(ExecutionResult {
        new_pc: None,
        should_halt: false,
        syscall: false,
        log: None,
    })
}

pub(super) fn execute_sub_fast(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    regs: &mut [i32; 32],
) -> Result<ExecutionResult, EmulatorError> {
    // ... fast implementation
}

// ============================================================================
// Logging Path (With Runtime Control)
// ============================================================================

pub(super) fn execute_add_logging(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    log_level: LogLevel,
) -> Result<ExecutionResult, EmulatorError> {
    let val1 = read_reg(regs, rs1);
    let val2 = read_reg(regs, rs2);
    let rd_old = read_reg(regs, rd);
    let result = val1.wrapping_add(val2);
    if rd.num() != 0 {
        regs[rd.num() as usize] = result;
    }
    let log = if log_level >= LogLevel::Instructions {
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

pub(super) fn execute_sub_logging(
    rd: Gpr,
    rs1: Gpr,
    rs2: Gpr,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    log_level: LogLevel,
) -> Result<ExecutionResult, EmulatorError> {
    // ... logging implementation
}

// ============================================================================
// Category Dispatch Functions
// ============================================================================

// Fast path: routes to individual instruction functions
pub(super) fn execute_arithmetic_fast(
    inst: Inst,
    _instruction_word: u32,
    _pc: u32,
    regs: &mut [i32; 32],
    _memory: &mut Memory,
) -> Result<ExecutionResult, EmulatorError> {
    match inst {
        Inst::Add { rd, rs1, rs2 } => execute_add_fast(rd, rs1, rs2, regs),
        Inst::Sub { rd, rs1, rs2 } => execute_sub_fast(rd, rs1, rs2, regs),
        Inst::Mul { rd, rs1, rs2 } => execute_mul_fast(rd, rs1, rs2, regs),
        // ... other arithmetic instructions delegate to their individual functions
        _ => unreachable!(),
    }
}

// Logging path: routes to individual instruction functions
pub(super) fn execute_arithmetic_logging(
    inst: Inst,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    _memory: &mut Memory,
    log_level: LogLevel,
) -> Result<ExecutionResult, EmulatorError> {
    match inst {
        Inst::Add { rd, rs1, rs2 } => {
            execute_add_logging(rd, rs1, rs2, instruction_word, pc, regs, log_level)
        }
        Inst::Sub { rd, rs1, rs2 } => {
            execute_sub_logging(rd, rs1, rs2, instruction_word, pc, regs, log_level)
        }
        Inst::Mul { rd, rs1, rs2 } => {
            execute_mul_logging(rd, rs1, rs2, instruction_word, pc, regs, log_level)
        }
        // ... other arithmetic instructions delegate to their individual functions
        _ => unreachable!(),
    }
}
```

**Benefits of this organization**:
- Fast and logging versions are side-by-side for easy comparison
- Clear separation with comments
- Each category in its own file for maintainability
- Easy to add new instruction categories (floating point, etc.)

## Conceptual Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Riscv32Emulator                                           │
│  ─────────────────────────────────────────────────────────  │
│  log_level: LogLevel                                       │
└─────────────────────────────────────────────────────────────┘
                    │
                    │ calls
                    ▼
┌─────────────────────────────────────────────────────────────┐
│  run_inner(fuel)                                            │
│  ─────────────────────────────────────────────────────────  │
│  match log_level {                                          │
│    LogLevel::None => run_inner_fast(fuel),                 │
│    _ => run_inner_logging(fuel),                           │
│  }                                                           │
└─────────────────────────────────────────────────────────────┘
        │                              │
        │                              │
        ▼                              ▼
┌──────────────────┐          ┌──────────────────────┐
│ run_inner_fast() │          │ run_inner_logging()  │
│ ──────────────── │          │ ──────────────────── │
│ • No logging     │          │ • Full logging       │
│ • Zero overhead  │          │ • Runtime control    │
└──────────────────┘          └──────────────────────┘
        │                              │
        │                              │
        ▼                              ▼
┌──────────────────┐          ┌──────────────────────┐
│ decode_execute   │          │ decode_execute        │
│ _fast()          │          │ _logging()           │
│ ──────────────── │          │ ──────────────────── │
│ • Checks opcode  │          │ • Checks opcode      │
│ • Routes to      │          │ • Routes to          │
│   category       │          │   category           │
│ • Calls fast     │          │ • Calls logging      │
│   functions      │          │   functions          │
└──────────────────┘          └──────────────────────┘
        │                              │
        │                              │
        ▼                              ▼
┌──────────────────┐          ┌──────────────────────┐
│ arithmetic_fast  │          │ arithmetic_logging    │
│ immediate_fast   │          │ immediate_logging     │
│ load_store_fast  │          │ load_store_logging    │
│ branch_fast      │          │ branch_logging        │
│ jump_fast        │          │ jump_logging         │
│ system_fast      │          │ system_logging        │
│ compressed_fast  │          │ compressed_logging    │
└──────────────────┘          └──────────────────────┘
```

## Implementation Strategy

### Phase 1: Create Dual Implementation Structure

1. Create `executor/` directory structure
2. Create `executor/mod.rs` with dispatch functions
3. Create `executor/arithmetic.rs` with fast and logging versions side-by-side
4. Implement one instruction (e.g., ADD) in both fast and logging versions
5. Update `run_loops.rs` to have `run_inner_fast()` and `run_inner_logging()`
6. Test that dispatch works correctly

### Phase 2: Decode-Execute Fusion

1. Create `decode_execute()` function that combines decode and execute
2. Start with most common instructions (arithmetic, immediate)
3. Use lookup tables for opcode dispatch
4. Gradually migrate from `decode()` + `execute()` to `decode_execute()`
5. Keep old API for backward compatibility during migration

### Phase 3: File Reorganization

1. Create new `executor/` directory structure
2. Move instructions into category files
3. Update imports throughout codebase
4. Add new instruction categories as needed (floating point, etc.)

## Technical Details

### Runtime Dispatch Implementation

The dispatch happens at runtime based on `log_level`:

```rust
// In executor/mod.rs

pub fn execute_instruction(
    inst: Inst,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
    log_level: LogLevel,
) -> Result<ExecutionResult, EmulatorError> {
    // Single runtime dispatch - compiler can optimize this
    match log_level {
        LogLevel::None => execute_instruction_fast(inst, instruction_word, pc, regs, memory),
        _ => execute_instruction_logging(inst, instruction_word, pc, regs, memory, log_level),
    }
}
```

**Benefits**:
- Zero overhead in fast path (no checks, no allocations)
- Runtime control when logging enabled (needed for filetests)
- Single dispatch point (compiler can optimize)
- No compile-time feature flags needed

### Alternative Approaches (Not Selected)

```rust
// Use const generic to monomorphize logging vs non-logging paths
pub trait LoggingMode {
    const ENABLED: bool;
}

pub struct LoggingEnabled;
pub struct LoggingDisabled;

impl LoggingMode for LoggingEnabled {
    const ENABLED: bool = true;
}

impl LoggingMode for LoggingDisabled {
    const ENABLED: bool = false;
}

pub fn execute_instruction<M: LoggingMode>(
    inst: Inst,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
    log_level: LogLevel,
) -> Result<ExecutionResult, EmulatorError>
where
    [(); M::ENABLED as usize]:,
{
    // Compiler will optimize away logging code when M::ENABLED is false
}
```

### Runtime Dispatch Control

**Approach** (Selected):
- Runtime dispatch: `log_level` field on `Riscv32Emulator` controls which path to use
- Fast path: When `log_level == LogLevel::None`, use `execute_instruction_fast()` - zero overhead
- Logging path: When `log_level != LogLevel::None`, use `execute_instruction_logging()` - full runtime control
- Single check at top level: `run_inner()` checks `log_level` once and calls appropriate loop

**Usage**:
```rust
// Set log level at runtime
emu.with_log_level(LogLevel::None);  // Fast path - zero overhead
emu.with_log_level(LogLevel::Instructions);  // Logging path - full control

// Dispatch happens automatically in run_inner()
emu.run_fuel(1000);  // Uses fast path if log_level == None
```

## Migration Path

1. **Phase 1**: Add macro system, refactor one instruction category
2. **Phase 2**: Measure and validate performance improvement
3. **Phase 3**: Refactor remaining instructions to use macros
4. **Phase 4**: Implement decode-execute fusion for hot path
5. **Phase 5**: Reorganize files into category structure
6. **Phase 6**: Add new instruction categories (floating point, etc.)

## Backward Compatibility

- Keep existing `execute_instruction()` API during migration
- Implement new `decode_execute()` alongside old functions
- Gradually migrate call sites
- Remove old API once migration complete

## Performance Targets

- **Logging overhead removal**: 15-25% improvement when logging disabled
- **Decode-execute fusion**: 10-15% improvement from reduced overhead
- **Combined**: 25-40% total improvement potential

## Testing Strategy

- Benchmark before/after for each phase
- Ensure logging still works when enabled
- Verify no functional changes (same instruction behavior)
- Test with various log levels
