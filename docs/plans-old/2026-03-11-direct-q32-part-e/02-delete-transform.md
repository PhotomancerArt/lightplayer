# Phase 2: Delete Transform Infrastructure

## Goal

Remove the entire `backend/transform/` directory and all references to it.

## Files to delete (27 files)

### `backend/transform/q32/` (17 files)
- `mod.rs`, `transform.rs`, `instructions.rs`, `signature.rs`
- `options.rs`, `types.rs` (originals, now duplicated in `backend/q32/`)
- `q32_test_util.rs`
- `converters/mod.rs`, `converters/arithmetic.rs`, `converters/boolean.rs`
- `converters/calls.rs`, `converters/comparison.rs`, `converters/constants.rs`
- `converters/conversions.rs`, `converters/helpers.rs`, `converters/math.rs`
- `converters/memory.rs`

### `backend/transform/identity/` (2 files)
- `mod.rs`, `transform.rs`

### `backend/transform/shared/` (5 files)
- `mod.rs`, `blocks.rs`, `function.rs`, `instruction_copy.rs`, `stack_slots.rs`
- `transform_test_util.rs`

### `backend/transform/` root (2 files)
- `mod.rs`, `pipeline.rs`

## Steps

1. Delete the entire `backend/transform/` directory

2. Remove `pub mod transform;` from `backend/mod.rs`

3. Fix compilation errors — references to the old transform paths:
   - `gl_module.rs` imports `Transform`, `TransformContext` — remove (phase 3)
   - `builtins-gen-app` — if it references `backend::transform::q32::converters::math`,
     update to use `backend::builtins::mapping` instead (this was already moved in Plan C)

4. `cargo check` — expect errors only in `gl_module.rs` (fixed in phase 3)
