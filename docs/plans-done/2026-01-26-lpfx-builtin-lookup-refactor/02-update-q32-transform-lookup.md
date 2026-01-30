# Phase 2: Update Q32 Transform to Use Proper Lookup Chain

## Description

Update `convert_call` in `calls.rs` to use the proper lookup chain for LPFX functions:
1. Look up `BuiltinId` from testcase name
2. Check if it's an LPFX function
3. If yes, use `LpfxFn` → `q32_impl` lookup
4. If no, fall back to `map_testcase_to_builtin`

## Implementation

1. Update `lp-glsl/crates/lp-glsl-compiler/src/backend/transform/q32/converters/calls.rs`:
   - In `convert_call`, when handling TestCase names:
     - Call `BuiltinId::builtin_id_from_name(testcase_name)`
     - If `Some(builtin_id)`, call `find_lpfx_fn_by_builtin_id(builtin_id)`
     - If `Some(lpfx_fn)`, extract `q32_impl` from `lpfx_fn.impls`
     - Use `q32_impl.name()` for the replacement
     - If not LPFX function, fall back to `map_testcase_to_builtin`
     - If neither works, return error

2. Import necessary functions:
   - `use crate::backend::builtins::registry::BuiltinId;`
   - `use crate::frontend::semantic::lpfx::lpfx_fn_registry::find_lpfx_fn_by_builtin_id;`

## Success Criteria

- Q32 transform uses proper lookup chain for LPFX functions
- No string manipulation for LPFX functions
- Fallback to `map_testcase_to_builtin` for regular q32 functions
- Error returned if neither lookup works
- Code compiles without errors
