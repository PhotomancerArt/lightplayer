# Phase 7: Update call sites to use new API where beneficial

## Scope of phase

Update call sites throughout the codebase to use the new `run()` or `run_fuel()` API where it makes sense, instead of calling `step()` in loops or using `run_until_yield()` with large fuel values.

## Code Organization Reminders

- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together

## Implementation Details

### 1. Identify call sites to update

Search for patterns like:
- `loop { match emu.step()? { ... } }` - Should use `run()` instead
- `emu.run_until_yield(very_large_number)` - Could use `run()` with default fuel
- Direct `step()` calls in loops - Consider if `run()` would be better

Key files to check:
- `lp-core/lp-client/src/transport_serial/emulator.rs` - Uses `run_until_yield(MAX_STEPS_PER_ITERATION)`
- `lp-core/lp-client/src/transport_serial_emu.rs` - Uses `run_until_yield(MAX_STEPS_PER_ITERATION)`
- `lp-riscv/lp-riscv-emu/src/emu/emulator/function_call.rs` - Uses `step()` in loops
- `lp-riscv/lp-riscv-elf/src/elf_loader/mod.rs` - Uses `step()` in loops
- Test files - Various uses

### 2. Update transport code

In `lp-core/lp-client/src/transport_serial/emulator.rs`:

```rust
// Old:
emu.run_until_yield(MAX_STEPS_PER_ITERATION)

// New option 1 (keep using run_until_yield, but it's now optimized):
emu.run_until_yield(MAX_STEPS_PER_ITERATION)

// New option 2 (use run() with default fuel, then check for yield):
loop {
    match emu.run()? {
        StepResult::Syscall(info) if info.number == SYSCALL_YIELD => {
            return Ok(info);
        }
        StepResult::FuelExhausted(_) => {
            // Continue with more fuel
            continue;
        }
        // ... handle other cases
    }
}
```

Actually, since `run_until_yield()` is already optimized (it uses `run()` internally), we can keep using it. The performance improvement comes from the tight loop in `run()`, not from changing the call site.

### 3. Update function_call.rs

In `lp-riscv/lp-riscv-emu/src/emu/emulator/function_call.rs`:

The `call_function()` method uses `step()` in a loop. We could potentially use `run()` here, but we need to stop at specific conditions (function return). Let's keep using `step()` for now since it's single-step debugging style code.

Actually, `function_call.rs` might benefit from using `run()` with a reasonable fuel limit, then checking if we've reached the return address. But that's a more complex change - let's leave it for now and focus on the high-impact changes.

### 4. Update test code

Update tests that use `step()` in loops unnecessarily:

```rust
// Old:
let mut steps = 0;
loop {
    match emu.step()? {
        StepResult::Halted => break,
        _ => steps += 1,
    }
}

// New:
loop {
    match emu.run()? {
        StepResult::Halted => break,
        StepResult::FuelExhausted(_) => continue, // Get more fuel
        _ => {},
    }
}
```

## Tests

All existing tests should continue to pass. We're just optimizing call sites, not changing behavior.

## Validate

Run:
```bash
cd lp-riscv/lp-riscv-emu
cargo check
cargo test
```

Also check dependent crates:
```bash
cd lp-core/lp-client
cargo check
cargo test
```

Ensure:
- Code compiles
- All tests pass
- Performance improvements are realized (can benchmark if desired)
- No regressions in functionality
