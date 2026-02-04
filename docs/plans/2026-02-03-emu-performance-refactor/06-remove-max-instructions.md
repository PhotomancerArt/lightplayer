# Phase 6: Remove max_instructions field and related methods

## Scope of phase

Remove the `max_instructions` field from `Riscv32Emulator` state and all related methods (`with_max_instructions()`, `set_max_instructions()`). Update any code that references these.

## Code Organization Reminders

- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together

## Implementation Details

### 1. Remove max_instructions field

In `lp-riscv/lp-riscv-emu/src/emu/emulator/state.rs`:

Remove:
- `pub(super) max_instructions: u64,` field
- Initialization: `max_instructions: 100_000,` in `with_traps()`
- `with_max_instructions()` method
- `set_max_instructions()` method

### 2. Update call sites

Search for all uses of:
- `with_max_instructions()`
- `set_max_instructions()`
- `max_instructions` field access

Update call sites to use `run_fuel()` instead, or remove the calls if they're setting a global limit that's no longer needed.

Key places to check:
- `lp-glsl/lp-glsl-compiler/src/backend/codegen/emu.rs` - Uses `with_max_instructions()`
- `lp-glsl/lp-glsl-compiler/src/exec/emu.rs` - May use `set_max_instructions()`
- Test files - May use `with_max_instructions()`

For example:
```rust
// Old:
let mut emu = Riscv32Emulator::new(code, ram)
    .with_max_instructions(1_000_000);

// New:
let mut emu = Riscv32Emulator::new(code, ram);
// Then use run_fuel(1_000_000) when calling run
```

### 3. Update error handling

The `EmulatorError::InstructionLimitExceeded` variant might still be used by `run_until_yield()` when fuel is exhausted. That's fine - we can keep the error variant, it's just that the limit is now per-run instead of global.

Actually, `run_until_yield()` returns `InstructionLimitExceeded` when fuel is exhausted, which is correct. The error still makes sense, just the semantics changed (per-run instead of global).

## Tests

Update any tests that use `with_max_instructions()` or `set_max_instructions()`:

```rust
// Old test:
let mut emu = Riscv32Emulator::new(code, ram)
    .with_max_instructions(1000);
emu.run_until_yield(1000);

// New test:
let mut emu = Riscv32Emulator::new(code, ram);
emu.run_until_yield(1000); // fuel is passed to run_until_yield
```

## Validate

Run:
```bash
cd lp-riscv/lp-riscv-emu
cargo check
cargo test
```

Also check dependent crates:
```bash
cd lp-glsl/lp-glsl-compiler
cargo check
cargo test
```

Ensure:
- Code compiles
- All tests pass
- No references to `max_instructions` field remain
- No references to `with_max_instructions()` or `set_max_instructions()` remain (except in error messages or comments if needed)
