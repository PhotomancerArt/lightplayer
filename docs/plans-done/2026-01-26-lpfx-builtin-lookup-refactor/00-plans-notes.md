# Plan: LPFX Builtin Lookup Refactor

## Problem Statement

The current implementation of LPFX function resolution and Q32 transform conversion uses hacky
string manipulation. The correct flow should be:

1. **Compiler frontend**: Resolves function call to `LpfxFn` → gets `float_impl` `BuiltinId` →
   generates call to `builtin_id.name()` (e.g., `__lpfx_saturate_vec3_f32`)
2. **Q32 Transform**: Sees `__lpfx_saturate_vec3_f32` in CLIF → looks up `BuiltinId` from name →
   finds corresponding `LpfxFn` → replaces with `q32_impl` `BuiltinId` → uses `builtin_id.name()` (
   e.g., `__lpfx_saturate_vec3_q32`)

Currently, the transform uses `map_testcase_to_builtin` which expects GLSL names without `_f32`
suffixes, requiring string manipulation. This is fragile and incorrect.

## Questions

### Q1: Where should `builtin_id_from_name` live?

**Context**: We need a function that maps a builtin name (e.g., `__lpfx_saturate_vec3_f32`) to a
`BuiltinId` enum variant.

**Answer**: Add `builtin_id_from_name(name: &str) -> Option<BuiltinId>` to
`lp-glsl/lp-glsl-compiler/src/backend/builtins/registry.rs` as an `impl BuiltinId` method.
This is the natural place since:

- `BuiltinId::name()` already exists there (reverse mapping)
- The registry is the source of truth for builtin IDs
- It's auto-generated, so we need to update the codegen tool

### Q2: How should the Q32 transform identify LPFX functions vs regular q32 functions?

**Context**: The transform needs to distinguish between:

- LPFX functions (e.g., `__lpfx_saturate_vec3_f32`) that need lookup via `BuiltinId` → `LpfxFn` →
  `q32_impl`
- Regular q32 functions (e.g., `__lp_q32_sin`) that can use `map_testcase_to_builtin` directly

**Answer**: Look up the `BuiltinId` from the name using `builtin_id_from_name`, then check if
`find_lpfx_fn_by_builtin_id` returns `Some`. If yes, it's an LPFX function and use the lookup chain.
If no, fall back to `map_testcase_to_builtin` for regular q32 functions.

### Q3: Should we remove `map_testcase_to_builtin` support for LPFX functions?

**Context**: Currently `map_testcase_to_builtin` in `math.rs` has match arms for LPFX functions like
`("__lpfx_saturate", 1) => Some(BuiltinId::LpfxSaturateQ32)`. If we use the proper lookup chain,
these become unnecessary.

**Answer**: Yes, remove LPFX function entries from `map_testcase_to_builtin`. Hardcoding these there
is wrong. Keep it only for regular q32 functions (e.g., `__lp_q32_sin`). Update the codegen tool to
stop generating LPFX entries in `map_testcase_to_builtin`.

### Q4: What happens if a builtin name doesn't map to an LPFX function?

**Context**: Some builtins (e.g., `LpQ32Sin`) are not LPFX functions. When we look up
`BuiltinId::LpQ32Sin` via `builtin_id_from_name`, `find_lpfx_fn_by_builtin_id` will return `None`.

**Answer**: If `find_lpfx_fn_by_builtin_id` returns `None`, treat it as a regular q32 function and
use `map_testcase_to_builtin`. If neither the LPFX lookup nor the regular q32 lookup works, it's an
error.

### Q5: Should we add unit tests for the lookup chain?

**Context**: The lookup chain (`name` → `BuiltinId` → `LpfxFn` → `q32_impl` → `name`) is critical
and should be tested.

**Answer**: Yes, add unit tests in `registry.rs` for `builtin_id_from_name` and in
`lpfx_fn_registry.rs` for the full lookup chain (f32 → q32 conversion).

## Notes

- The codegen tool (`lp-glsl-builtin-gen-app`) needs to generate `builtin_id_from_name` in
  `registry.rs`
- The Q32 transform in `calls.rs` needs to be updated to use the proper lookup chain
- We should verify that all existing tests still pass after the refactor
