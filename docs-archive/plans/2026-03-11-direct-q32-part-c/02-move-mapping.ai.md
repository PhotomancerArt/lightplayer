# Phase 2: Move map_testcase_to_builtin

## Rationale

`map_testcase_to_builtin` maps float libcall names (e.g. "sinf") to Q32
BuiltinIds (e.g. `LpQ32Sin`). It currently lives in
`backend/transform/q32/converters/math.rs` — part of the transform we
intend to remove in Plan E.

The frontend codegen needs this mapping (Phase 3). Moving it to
`backend/builtins/` ensures it survives transform removal and lives
alongside the BuiltinId registry where it belongs.

## Changes

1. Move `map_testcase_to_builtin` function from
   `backend/transform/q32/converters/math.rs` to a new or existing file
   in `backend/builtins/` (e.g. `backend/builtins/mapping.rs`).
2. Re-export from `backend/builtins/mod.rs`.
3. Update the import in `backend/transform/q32/converters/calls.rs` to
   use the new location.
4. The tests for this function (`test_map_testcase_to_builtin_*`) move
   with it.

## Validate

```bash
cargo check -p lps-compiler --features std
cargo test -p lps-compiler --features std -- map_testcase
```
