# Phase 5: Cleanup & Validation

## Scope

Clean up any temporary code, fix warnings, ensure all tests pass, and complete the plan.

## Cleanup Checklist

### Remove Temporary Code

Search for and remove:
- `TODO` comments related to quick fixes
- `println!` or `eprintln!` debug statements
- `#[allow(dead_code)]` that was added temporarily
- Any `unimplemented!()` or `todo!()` macros (should all be resolved)

### Fix Warnings

```bash
cargo check -p lp-glsl-naga
cargo clippy -p lp-glsl-naga -- -D warnings
```

Common issues to fix:
- Unused imports
- Unused variables
- Dead code
- Missing documentation on public items

### Format Code

```bash
cargo +nightly fmt -p lp-glsl-naga
```

## Validation

### Test All Three Phases

```bash
# Phase 1: Foundation
scripts/glsl-filetests.sh array/phase/1-foundation.glsl

# Phase 2: Bounds Checking  
scripts/glsl-filetests.sh array/phase/2-bounds-checking.glsl

# Phase 3: Initialization
scripts/glsl-filetests.sh array/phase/3-initialization.glsl
```

Each should show all tests passing.

### Regression Testing

Ensure existing tests still pass:

```bash
cargo test -p lp-glsl-filetests --test filetests -- jit.q32
```

Verify no new unexpected failures.

### Check Firmware Build

Per the `.cursorrules`, ensure embedded builds work:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

## Summary Update

Create `docs/plans/2026-03-30-lpir-parity-stage-iv/summary.md`:

```markdown
# Array Lowering Implementation Summary

## Completed Work

Implemented 1D scalar array support in LPIR for GLSL, covering Phases 1-3 of array test files.

### Phase 1: Declaration & Slot Allocation
- Added `ArrayInfo` struct and `array_map` to track array metadata
- Modified `LowerCtx::new()` to allocate LPIR slots for array-typed locals
- Added array type utilities to `naga_util.rs`

### Phase 2: Constant Indexing  
- Implemented `AccessIndex` lowering for arrays
- Added `Store` support for array element assignment
- Uses constant offset computation: `offset = index * element_size`

### Phase 3: Dynamic Indexing with Bounds Clamping
- Created `lower_array.rs` with array-specific lowering helpers
- Implemented `clamp_index()` using select chains
- Added `Access` lowering for dynamic array access
- Added dynamic `Store` support with clamping

### Phase 4: Initialization
- Implemented array initializer list lowering
- Full initialization: store all elements
- Partial initialization: store given elements + zero-fill remainder
- Unsized arrays: handled by Naga size inference

## Files Modified

- `lp-glsl/lp-glsl-naga/src/lower_ctx.rs` - Array metadata tracking
- `lp-glsl/lp-glsl-naga/src/lower_expr.rs` - Array read access
- `lp-glsl/lp-glsl-naga/src/lower_stmt.rs` - Array store
- `lp-glsl/lp-glsl-naga/src/naga_util.rs` - Array type utilities  
- `lp-glsl/lp-glsl-naga/src/lower_array.rs` - NEW: Array helpers
- `lp-glsl/lp-glsl-naga/src/lib.rs` - Export new module
- `lp-glsl/lp-glsl-filetests/filetests/array/phase/2-bounds-checking.glsl` - Updated for clamping

## Test Results

- `array/phase/1-foundation.glsl`: 6/6 tests passing
- `array/phase/2-bounds-checking.glsl`: 10/10 tests passing  
- `array/phase/3-initialization.glsl`: 5/5 tests passing

## Design Decisions Applied

1. **Stack slots uniformly** - All arrays stored in LPIR slots
2. **Bounds clamping** - OOB accesses clamp to valid range (v1 safety)
3. **Select chains** - Used for clamping instead of branches
4. **Separate array_map** - Keeps non-array path simple

## Known Limitations

- Clamping may produce incorrect results silently (documented)
- Future work: Trap + stack unwinding for better error handling
- Arrays of vectors/matrices: deferred to later phase
- Function parameters: deferred to Phase 7
- Multi-dimensional arrays: deferred
```

## Plan Cleanup

Move the plan to done:

```bash
mv docs/plans/2026-03-30-lpir-parity-stage-iv docs/plans-done/
```

## Commit

Commit message:

```
feat(lp-glsl): implement 1D scalar array lowering

Add array support to LPIR lowering:
- Array declaration with slot allocation
- Constant and dynamic element indexing
- Bounds clamping for safety on embedded targets
- Array initialization (full, partial, unsized)

Files added:
- lower_array.rs: array-specific lowering helpers

Files modified:
- lower_ctx.rs: array_map for metadata tracking
- lower_expr.rs: Access/AccessIndex for arrays
- lower_stmt.rs: Store support for array elements
- naga_util.rs: array type utilities

Tests updated:
- 2-bounds-checking.glsl: clamping behavior

All 21 tests in phases 1-3 now passing.
```
