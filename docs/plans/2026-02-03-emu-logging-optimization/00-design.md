# Emulator Logging and Decode-Execute Optimization - Design

## Scope of Work

Improve emulator performance by:
1. **Eliminate logging overhead when disabled** - Use compile-time feature flags/macros to remove logging code entirely when not needed
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

### 1. Compile-Time Logging Macros

Use Rust macros to conditionally compile logging code based on a compile-time feature flag.

**Approach**: Create a macro system that:
- Generates two versions of execute functions: with logging and without
- Uses feature flags to select which version to compile
- Maintains runtime log level control for when logging IS enabled

**Macro Design**:
```rust
// Macro that conditionally includes logging code
macro_rules! with_logging {
    ($log_level:expr, $log_expr:expr, $no_log_expr:expr) => {
        #[cfg(feature = "logging")]
        {
            if $log_level != LogLevel::None {
                $log_expr
            } else {
                $no_log_expr
            }
        }
        #[cfg(not(feature = "logging"))]
        {
            $no_log_expr
        }
    };
}

// Or use const generics for monomorphization
pub fn execute_instruction<const LOGGING: bool>(
    inst: Inst,
    instruction_word: u32,
    pc: u32,
    regs: &mut [i32; 32],
    memory: &mut Memory,
    log_level: LogLevel,
) -> Result<ExecutionResult, EmulatorError>
```

**Better Approach**: Use const generics with trait-based dispatch
- `execute_instruction_fast()` - no logging, always compiled
- `execute_instruction_with_logging()` - with logging, only compiled if feature enabled
- Runtime selection between them based on log_level

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

Split instructions into separate modules:

```
lp-riscv/lp-riscv-emu/src/emu/executor/
├── mod.rs                    # Main decode_execute entry point
├── arithmetic.rs             # ADD, SUB, MUL, etc.
├── immediate.rs              # ADDI, SLLI, SRLI, etc.
├── load_store.rs             # LW, SW, LB, etc.
├── branch.rs                 # BEQ, BNE, BLT, etc.
├── jump.rs                   # JAL, JALR
├── system.rs                 # ECALL, EBREAK, CSR
├── compressed.rs             # C.ADD, C.LW, etc.
├── floating_point.rs         # Future: FADD, FMUL, etc.
└── macros.rs                 # Logging macros and helpers
```

Each file contains:
- Decode-execute function for that instruction category
- Instruction-specific optimizations
- Logging code (conditionally compiled)

## Implementation Strategy

### Phase 1: Macro System for Logging

1. Create `executor/macros.rs` with logging macros
2. Add `logging` feature flag to `Cargo.toml`
3. Refactor one instruction type (e.g., arithmetic) to use macros
4. Measure performance improvement
5. Refactor remaining instructions

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

### Logging Macro Implementation

```rust
// executor/macros.rs

/// Execute with conditional logging
/// When logging is disabled at compile-time, this becomes a no-op
#[macro_export]
macro_rules! execute_with_log {
    ($log_level:expr, $log_fn:expr) => {
        #[cfg(feature = "logging")]
        {
            if $log_level != LogLevel::None {
                $log_fn()
            } else {
                None
            }
        }
        #[cfg(not(feature = "logging"))]
        {
            None
        }
    };
}

/// Read register value for logging (only if logging enabled)
#[macro_export]
macro_rules! read_reg_for_log {
    ($log_level:expr, $regs:expr, $reg:expr) => {
        #[cfg(feature = "logging")]
        {
            if $log_level != LogLevel::None {
                Some(read_reg($regs, $reg))
            } else {
                None
            }
        }
        #[cfg(not(feature = "logging"))]
        {
            None
        }
    };
}
```

### Const Generic Approach (Alternative)

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

### Runtime vs Compile-Time Control

**Hybrid Approach** (Recommended):
- Compile-time feature flag: `logging` feature controls whether logging code is compiled
- Runtime log level: When logging IS compiled, use runtime `LogLevel` to control verbosity
- This gives us: zero overhead when disabled, flexible control when enabled

**Cargo.toml**:
```toml
[features]
default = []
logging = []  # Enable logging support (adds code size, but allows runtime control)
std = ["env_logger", "log/std"]
```

**Usage**:
```rust
// When logging feature is disabled: zero overhead, no logging possible
// When logging feature is enabled: runtime LogLevel controls verbosity
let result = execute_instruction(inst, word, pc, regs, memory, LogLevel::None);
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
