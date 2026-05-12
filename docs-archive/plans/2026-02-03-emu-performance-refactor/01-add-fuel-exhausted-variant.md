# Phase 1: Add FuelExhausted variant to StepResult

## Scope of phase

Add `FuelExhausted(u64)` variant to the existing `StepResult` enum. This variant will be returned by `run()` functions when fuel is exhausted, but never by `step()`.

## Code Organization Reminders

- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together

## Implementation Details

### 1. Update StepResult enum

In `lp-riscv/lp-riscv-emu/src/emu/emulator/types.rs`:

```rust
/// Result of a single step.
#[derive(Debug, Clone)]
pub enum StepResult {
    /// Normal step completed, continue execution
    Continue,
    /// ECALL encountered, syscall information available
    Syscall(SyscallInfo),
    /// EBREAK encountered, execution halted
    Halted,
    /// Trap encountered with trap code
    Trap(TrapCode),
    /// Panic occurred, panic information available
    Panic(PanicInfo),
    /// Fuel exhausted during run (instructions executed in this run)
    /// Only returned by run() functions, never by step()
    FuelExhausted(u64),
}
```

### 2. Update any match statements that need to handle FuelExhausted

Search for all `match` statements on `StepResult` and ensure they handle the new variant (or use `_` catch-all). Most existing code won't need changes since `step()` never returns `FuelExhausted`, but we should verify.

Key places to check:
- `run_loops.rs` - `run_until_*()` functions
- `function_call.rs` - function calling code
- Test files

For now, existing code can use `_` to catch `FuelExhausted` if needed, or explicitly handle it if the code path could receive it from `run()`.

## Tests

No new tests needed for this phase - we're just adding a variant. Tests will be added in later phases when we implement `run()`.

## Validate

Run:
```bash
cd lp-riscv/lp-riscv-emu
cargo check
cargo test
```

Ensure:
- Code compiles
- All existing tests pass
- No warnings about non-exhaustive matches (if any match statements need updating)
