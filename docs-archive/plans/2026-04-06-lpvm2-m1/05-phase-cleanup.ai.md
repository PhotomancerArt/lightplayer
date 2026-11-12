# Phase 5: Cleanup and Validation

## Scope

Final cleanup: fix any warnings, ensure formatting, verify all tests pass.
Add any missing documentation, remove temporary code markers.

## Code Organization Reminders

- No `TODO` comments left in final code
- No unused imports or dead code warnings
- All public items have documentation

## Implementation Details

### Check for Temporary Markers

```bash
grep -n "TODO" lp-shader/lpvm/src/*.rs
```

Remove any temporary markers that were added during development.

### Verify Documentation Coverage

All public items should have doc comments:
- `ShaderPtr` and its methods
- `AllocError` and its variants
- `LpvmMemory` trait and all methods
- `LpvmEngine::memory()` method

### Final Formatting

```bash
cargo +nightly fmt
```

## Validate

Run the full validation suite:

```bash
# Check compiles without errors or warnings
cargo check -p lpvm 2>&1 | grep -i "warning\|error" || echo "Clean build"

# Run all tests
cargo test -p lpvm

# Verify documentation tests
cargo test -p lpvm --doc

# Check formatting
cargo +nightly fmt --check

# Verify public API documentation coverage
cargo doc -p lpvm --no-deps
```

## Summary

This milestone adds shared memory support to the LPVM traits:

1. **`ShaderPtr`** — dual native/guest pointer for shared memory access
2. **`AllocError`** — concrete error type for allocation failures
3. **`LpvmMemory`** — object-safe trait for shared memory allocators
4. **`LpvmEngine::memory()`** — exposes the engine's shared memory
5. **Documentation** — clarifies per-instance vs shared data architecture

No backend implementations are updated in this milestone — that work is
deferred to M2 (WASM), M3 (Cranelift), and M4 (emulator). This milestone
establishes the trait foundation that all backends will implement.
