# Emulator Performance Refactor - Notes

## Scope of Work

Improve emulator performance by consolidating and refactoring how the emulator is used. The main goals are:

1. **Rename `max_instructions` to `fuel`** - Better semantic meaning
2. **Create a clean `run(fuel)` function** - Simple interface that takes fuel and runs until fuel is exhausted or a yield/halt occurs
3. **Implement tight loop with inline fuel checking** - Move fuel checking into the hot loop to reduce function call overhead
4. **Consolidate emulator usage patterns** - Currently fuel/instruction limits are handled inconsistently across the codebase

### Performance Target

The user specifically wants to implement a tight loop pattern similar to embive's `run()` method:
- Move fuel checking into the loop itself: `fuel--; if fuel == 0 { return }`
- Use `#[inline(always)]` on hot path functions (`step()`, `fetch_instruction()`, `decode_execute()`)
- Use `likely()`/`unlikely()` hints for branch prediction
- Reduce Result unwrapping overhead in the hot path

**Expected Impact**: 20-30% improvement from reduced function call overhead

## Current State of the Codebase

### Fuel/Instruction Limit Management

Currently, the emulator uses `max_instructions` and `instruction_count`:

1. **State fields** (`state.rs`):
   - `instruction_count: u64` - Tracks instructions executed
   - `max_instructions: u64` - Maximum allowed instructions (default: 100_000)

2. **Current usage patterns**:
   - `step()` checks `instruction_count >= max_instructions` at the start
   - `run_until_yield()` **resets** `instruction_count = 0` and **overwrites** `max_instructions` (sloppy!)
   - `function_call()` also resets `instruction_count = 0` in some cases
   - Various places call `set_max_instructions()` to change limits

3. **Problems**:
   - `run_until_yield()` mutates global state (`instruction_count`, `max_instructions`)
   - No clear separation between "fuel for this run" vs "global limit"
   - Fuel checking happens in `step()` which adds overhead on every instruction
   - Multiple ways to set limits (builder pattern, mutating method, direct field access)

### Instruction Execution Flow

1. **`execution.rs::step()`** - Main entry point
   - Checks `instruction_count >= max_instructions` (fuel check)
   - Fetches instruction
   - Decodes instruction
   - Increments `instruction_count`
   - Executes instruction
   - Updates PC
   - Handles logging, syscalls, traps, panics

2. **Hot path overhead**:
   - Function call to `step()` for every instruction
   - Result unwrapping (`match self.step()?`)
   - Fuel check happens inside `step()` (function call overhead)
   - Error handling captures full register state (only on error path, acceptable)

### Current Run Functions

1. **`run_until_ebreak()`** - Runs until EBREAK, returns a0 value
2. **`run_until_ecall()`** - Runs until ECALL, returns syscall info
3. **`run_until_yield()`** - Runs until SYSCALL_YIELD, mutates global state

All of these call `step()` in a loop, which has overhead.

### Usage Sites

- `transport_serial_emu.rs` - Calls `run_until_yield(MAX_STEPS_PER_ITERATION)`
- `transport_serial/emulator.rs` - Calls `run_until_yield(MAX_STEPS_PER_ITERATION)`
- `function_call.rs` - Calls `step()` in loops
- `elf_loader/mod.rs` - Calls `step()` in loops
- Various tests - Call `step()` or `run_until_*()` functions

## Questions

1. **Should `fuel` be a parameter to `run()` or a field on the emulator?**
   - **Answer**: No global safety limit. Fuel is per-run only. Provide:
     - `run()` - Uses default fuel (e.g., 100_000)
     - `run_fuel(fuel)` - Uses specified fuel amount
     - This keeps it simple - each run has its own fuel, no global state to manage

2. **Should we keep `instruction_count` as a cumulative counter or reset it per `run()`?**
   - **Answer**: Keep `instruction_count` as cumulative (total instructions executed since emulator creation). This is useful for performance metrics and debugging. The per-run fuel decrements independently and doesn't affect the cumulative counter.

3. **What should `run()` return?**
   - **Answer**: `run()` should return `Result<StepResult, EmulatorError>`. Add `FuelExhausted(u64)` variant to existing `StepResult` enum. This avoids duplicating the enum - `step()` never returns `FuelExhausted` (only `run()` can), but reusing the enum is cleaner than creating a separate `RunResult` type.

4. **Should we keep the existing `run_until_*()` functions or replace them?**
   - **Answer**: Keep them for backward compatibility, but reimplement them in terms of `run()` or `run_fuel()` with appropriate fuel values. They can use the default fuel allocation. This allows gradual migration and maintains existing API while benefiting from performance improvements.

5. **How should we handle the tight loop implementation?**
   - **Answer**: Create an internal `run_inner(fuel)` function that implements the tight loop. The public `run()` and `run_fuel()` wrap it for error handling. The tight loop should:
     - Inline fuel checking: `fuel -= 1; if fuel == 0 { return FuelExhausted }`
     - Use `#[inline(always)]` on `step_inner()` (internal version without fuel check)
     - Use `likely()`/`unlikely()` on the continue path vs yield/halt paths
     - Keep error handling outside the hot loop where possible
   - Structure:
     - `step()` - Public API, calls `step_inner()` directly (no fuel check, since no global fuel)
     - `step_inner()` - Internal, no fuel check, `#[inline(always)]`
     - `run_inner(fuel)` - Tight loop that calls `step_inner()` and checks fuel inline
     - `run()` / `run_fuel()` - Public API, wraps `run_inner()` for error handling

6. **Should `step()` remain public or become internal?**
   - **Answer**: Keep `step()` public for single-step debugging and backward compatibility. Since there's no global fuel, `step()` should just call `step_inner()` directly without fuel checking. Fuel checking only happens in `run()` loops where we have a specific fuel amount for that run.

7. **What about `instruction_count` vs `fuel` naming?**
   - **Answer**: 
     - Remove `max_instructions` entirely (no global limit)
     - Keep `instruction_count` as cumulative counter (for stats/debugging)
     - `run()` and `run_fuel(fuel)` take fuel per-run
     - This makes it clear: `fuel` is "how much can I run this time", `instruction_count` is "how much have I run total"
