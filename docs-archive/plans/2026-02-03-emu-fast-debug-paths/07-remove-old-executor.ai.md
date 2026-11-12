# Phase 7: Remove Old executor.rs

## Scope of Phase

Remove the old `executor.rs` file and clean up any remaining references to the old `execute_instruction()` function. Verify all call sites have been migrated to the new `decode_execute<M>()` function.

## Code Organization Reminders

- Clean up any temporary code or TODOs
- Remove unused imports
- Update any remaining references

## Implementation Details

### 1. Search for Remaining References

Search for any remaining uses of the old `execute_instruction()` function:

```bash
cd lp-riscv/lp-riscv-emu
grep -r "execute_instruction" src/
```

### 2. Update Any Remaining Call Sites

If any call sites remain, update them to use `decode_execute<M>()` instead.

### 3. Remove executor.rs

Delete `lp-riscv/lp-riscv-emu/src/emu/executor.rs`:

```bash
rm lp-riscv/lp-riscv-emu/src/emu/executor.rs
```

### 4. Update mod.rs if Needed

Check `lp-riscv/lp-riscv-emu/src/emu/mod.rs` to ensure it doesn't reference the old `executor.rs`:

```rust
// Should have:
pub mod executor;  // This now refers to executor/mod.rs

// Should NOT have:
// pub mod executor;  // old executor.rs
```

### 5. Clean Up Imports

Remove any imports of the old executor module that are no longer needed.

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
- No references to old `executor.rs` remain
- All code compiles
- All tests pass
- No warnings about unused code
