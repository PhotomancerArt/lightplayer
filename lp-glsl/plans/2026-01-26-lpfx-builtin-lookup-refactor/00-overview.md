# Plan: LPFX Builtin Lookup Refactor

## Overview

Refactor the Q32 transform to use proper lookup chains instead of hacky string manipulation. The correct flow is:

1. **Compiler frontend**: Resolves function call to `LpfxFn` → gets `float_impl` `BuiltinId` → generates call to `builtin_id.name()` (e.g., `__lpfx_saturate_vec3_f32`)
2. **Q32 Transform**: Sees `__lpfx_saturate_vec3_f32` in CLIF → looks up `BuiltinId` from name → finds corresponding `LpfxFn` → replaces with `q32_impl` `BuiltinId` → uses `builtin_id.name()` (e.g., `__lpfx_saturate_vec3_q32`)

Currently, the transform uses `map_testcase_to_builtin` which expects GLSL names without `_f32` suffixes, requiring string manipulation. This is fragile and incorrect.

## Phases

1. Update codegen tool to generate `builtin_id_from_name` function
2. Update Q32 transform to use proper lookup chain for LPFX functions
3. Remove LPFX entries from `map_testcase_to_builtin` in codegen tool
4. Add unit tests for lookup chain
5. Regenerate builtins and verify all tests pass
6. Cleanup and finalization

## Success Criteria

- No string manipulation in Q32 transform for LPFX functions
- Proper lookup chain used: `name` → `BuiltinId` → `LpfxFn` → `q32_impl` → `name`
- All existing tests pass
- Codegen tool generates `builtin_id_from_name` correctly
- `map_testcase_to_builtin` no longer contains LPFX function entries
