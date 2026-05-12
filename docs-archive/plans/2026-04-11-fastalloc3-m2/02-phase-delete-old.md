# Phase 2: Delete old code

## Goal

Remove `rv32/alloc.rs`, stub functions, and update error types.

## Changes

### Delete `rv32/alloc.rs`
- Remove the entire file

### `rv32/mod.rs`
- Remove `pub mod alloc;` line

### `error.rs`
- Change `FastAlloc(crate::rv32::alloc::AllocError)` → `FastAlloc(crate::fa_alloc::AllocError)`
- Update `Display` impl if needed (should be transparent since both have `Display`)

### `fa_alloc/mod.rs`
- Remove `run_shell` function
- Remove tests that reference `walk_region_stub`

### `fa_alloc/walk.rs`
- Remove `walk_region_stub` function

### `fa_alloc/trace.rs`
- Remove `stub_entry` function
- Remove `stub_detail` if it exists
- Remove tests that use `stub_entry`

### Test updates
- Any test in `fa_alloc/mod.rs` that calls `walk_region_stub` or `run_shell`
  needs to be removed or updated to use the real `allocate` path
- `compile.rs` tests should still pass since they go through `compile_function`

## Status: [ ]
