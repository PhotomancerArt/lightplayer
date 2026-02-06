# Emulator Performance Refactor - Design

## Scope of Work

Refactor the emulator to improve performance by:
1. Renaming `max_instructions` to `fuel` concept (per-run, no global limit)
2. Creating clean `run()` and `run_fuel()` functions
3. Implementing tight loop with inline fuel checking for 20-30% performance improvement
4. Consolidating emulator usage patterns

## File Structure

```
lp-riscv/lp-riscv-emu/src/emu/emulator/
├── mod.rs                    # (no changes needed)
├── types.rs                  # UPDATE: Add FuelExhausted variant to StepResult
├── state.rs                  # UPDATE: Remove max_instructions field, keep instruction_count
├── execution.rs              # UPDATE: Refactor step() to call step_inner(), remove fuel check
│                              # NEW: Add step_inner() - internal, no fuel check, #[inline(always)]
├── run_loops.rs              # UPDATE: Add run() and run_fuel() with tight loop
│                              # NEW: Add run_inner() - tight loop implementation
│                              # UPDATE: Reimplement run_until_*() in terms of run()
└── ... (other files unchanged)
```

## Conceptual Architecture

### Fuel Management

```
┌─────────────────────────────────────────┐
│  Riscv32Emulator                       │
│  ─────────────────────────────────────  │
│  instruction_count: u64 (cumulative)   │  ← Total instructions executed
│  (no max_instructions field)          │
└─────────────────────────────────────────┘
              │
              │ calls
              ▼
┌─────────────────────────────────────────┐
│  run(fuel: u64)                         │  ← Default fuel (100_000)
│  run_fuel(fuel: u64)                    │  ← Specified fuel
│  ─────────────────────────────────────  │
│  • Creates local fuel counter          │
│  • Calls run_inner(fuel)               │
│  • Returns StepResult                   │
└─────────────────────────────────────────┘
              │
              │ calls
              ▼
┌─────────────────────────────────────────┐
│  run_inner(fuel: u64)                   │  ← Tight loop (hot path)
│  ─────────────────────────────────────  │
│  loop {                                 │
│    fuel -= 1;                           │  ← Inline fuel check
│    if fuel == 0 {                       │
│      return FuelExhausted;              │
│    }                                    │
│    match step_inner()? {                │  ← No fuel check here
│      Continue => continue,              │
│      Yield(info) => return Yield(info), │
│      Halted => return Halted,           │
│      ...                                 │
│    }                                    │
│  }                                      │
└─────────────────────────────────────────┘
              │
              │ calls
              ▼
┌─────────────────────────────────────────┐
│  step_inner()                           │  ← #[inline(always)]
│  ─────────────────────────────────────  │
│  • Fetch instruction                     │
│  • Decode instruction                    │
│  • Execute instruction                   │
│  • Update PC                            │
│  • Increment instruction_count           │
│  • Return StepResult                     │
└─────────────────────────────────────────┘
```

### Public API

```rust
// New primary API
pub fn run(&mut self) -> Result<StepResult, EmulatorError>
pub fn run_fuel(&mut self, fuel: u64) -> Result<StepResult, EmulatorError>

// Backward compatibility (reimplemented using run())
pub fn run_until_ebreak(&mut self) -> Result<i32, EmulatorError>
pub fn run_until_ecall(&mut self) -> Result<SyscallInfo, EmulatorError>
pub fn run_until_yield(&mut self, max_steps: u64) -> Result<SyscallInfo, EmulatorError>

// Single-step (unchanged API, optimized implementation)
pub fn step(&mut self) -> Result<StepResult, EmulatorError>
```

### StepResult Extension

Add `FuelExhausted` variant to existing `StepResult` enum:

```rust
pub enum StepResult {
    Continue,                 // Normal step completed (used in loops)
    Syscall(SyscallInfo),    // ECALL encountered
    Halted,                   // EBREAK encountered (not a trap)
    Trap(TrapCode),           // Trap encountered
    Panic(PanicInfo),         // Panic occurred
    FuelExhausted(u64),       // NEW: Fuel ran out (instructions executed in this run)
}
```

Note: `step()` never returns `FuelExhausted` (only `run()` can), but reusing the enum avoids duplication.

## Main Components and Interactions

### 1. State Management
- **`instruction_count`**: Cumulative counter, never reset
- **Removed `max_instructions`**: No global fuel limit
- Fuel is passed as parameter to `run()` functions

### 2. Execution Flow

**Hot Path (run_inner)**:
1. Tight loop with inline fuel decrement: `fuel -= 1; if fuel == 0 { return }`
2. Calls `step_inner()` which is `#[inline(always)]`
3. Uses `likely()`/`unlikely()` hints on branch prediction
4. Minimal error handling overhead in hot path

**Single Step (step)**:
1. Public API for debugging
2. Calls `step_inner()` directly (no fuel check)
3. Returns `StepResult` for backward compatibility

### 3. Performance Optimizations

1. **Inline Fuel Checking**: Fuel decrement happens in the loop, not in function calls
2. **Inline Hot Functions**: `step_inner()`, `fetch_instruction()`, `decode_execute()` marked `#[inline(always)]`
3. **Branch Prediction Hints**: `likely()` on Continue path, `unlikely()` on Yield/Halted paths
4. **Reduced Function Call Overhead**: Single function call per instruction instead of nested calls
5. **Result Handling**: Error handling outside hot loop where possible

### 4. Backward Compatibility

- `run_until_*()` functions reimplemented using `run()` with appropriate fuel
- `step()` maintains same API, optimized implementation
- All existing code continues to work

## Migration Strategy

1. Add new `RunResult` type and `run()`/`run_fuel()` functions
2. Implement `step_inner()` and refactor `step()` to use it
3. Implement `run_inner()` with tight loop
4. Reimplement `run_until_*()` functions using `run()`
5. Remove `max_instructions` field and related methods
6. Update call sites to use new API where beneficial
