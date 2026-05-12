# Phase 6: Cleanup & Validation

## Scope

Final cleanup: fix all warnings, ensure clean compilation, document temporary
code with TODO comments, add phase summary.

## Cleanup Checklist

### 1. Fix Warnings

```bash
cargo check -p lpvm-native --lib 2>&1 | grep -E "^warning:" | head -30
```

For each warning:
- Dead code: Add `#[allow(dead_code)]` if it's intentionally temporary, or remove
- Unused imports: Remove them
- Unused variables: Prefix with `_` or remove
- Missing docs: Add doc comments for public items

### 2. Add TODO Comments

Mark all temporary/stub code:

```rust
// TODO(M2): Implement real backward walk allocator
pub fn allocate(...) -> Result<AllocOutput, AllocError> {
    Err(AllocError::NotImplemented)
}

// TODO(M2): Implement full VInst emission
fn emit_vinst(...) -> Result<(), AllocError> {
    Ok(())
}

// TODO(M4): Implement branch fixup resolution
fn resolve_branch_fixups(...) -> Result<(), AllocError> {
    Ok(())
}
```

### 3. Check Documentation

Ensure public types have doc comments:

- `Alloc` enum
- `AllocOutput` struct
- `EditPoint` enum
- `Edit` enum
- `RegPool` struct
- `EmitContext` struct
- `emit_function`

### 4. Verify No Old References

Search for remnants of deleted code:

```bash
grep -r "PInst" lp-shader/lpvm-native/src/ 2>/dev/null || echo "None found - good"
grep -r "rv32_emit" lp-shader/lpvm-native/src/ 2>/dev/null || echo "None found - good"
grep -r "walk_region" lp-shader/lpvm-native/src/ 2>/dev/null || echo "None found - good"
```

### 5. Check Tests Compile

```bash
cargo test -p lpvm-native --no-run 2>&1 | tail -10
```

Expected: Tests compile (though they may not all pass yet).

## Validation Commands

Final validation:

```bash
# Clean check
cargo check -p lpvm-native

# Check with all features
cargo check -p lpvm-native --all-features

# Check tests compile
cargo test -p lpvm-native --no-run

# Check no_std build (for firmware)
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf 2>&1 | head -20
```

## Summary

At the end of M1:

- ✅ All broken code deleted (walk.rs, inst.rs, rv32_emit.rs, pinst.rs)
- ✅ New types defined (Alloc, AllocOutput, Edit, EditPoint)
- ✅ RegPool extracted and tested
- ✅ Forward emitter skeleton ported
- ✅ Orchestration wired to new types
- ✅ Clean compilation with `cargo check`
- ✅ All TODOs documented with phase references

## Notes

The allocator is intentionally stubbed — it returns `Err(NotImplemented)`.
This is expected. M2 will implement the real backward walk that produces
per-operand allocations and the edit list.

The emitter has stubbed methods for `emit_vinst` and `resolve_branch_fixups`.
These will be completed as the allocator becomes functional.
