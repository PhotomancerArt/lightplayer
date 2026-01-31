# Plan: LP Library Float-to-Q32 Conversion

## Overview

Fix LP library functions (`lpfx_snoise1/2/3`, `lpfx_hash`) to follow the correct float→q32
conversion pattern. Currently, codegen directly calls builtins, bypassing the transform. Functions
should emit TestCase calls that the q32 transform converts, matching the pattern used for `sin`/
`cos`.

## Phases

1. **Extend LpLibFn with q32 mapping methods** - Add `needs_q32_mapping()` and `q32_name()` methods
   to determine conversion requirements
2. **Update codegen to emit TestCase calls** - Change `emit_lp_lib_fn_call()` to emit TestCase calls
   for functions that need q32 mapping
3. **Fix generator to use LpLibFn as source of truth** - Update `lp-glsl-builtin-gen-app` to read
   `LpLibFn` enum instead of using prefix matching
4. **Regenerate builtin registry** - Run generator to create correct `BuiltinId` variants (
   LpSimplex3, not Q32LpSimplex3)
5. **Verify transform mappings** - Ensure `map_testcase_to_builtin()` correctly maps TestCase names
   to BuiltinIds
6. **Test end-to-end flow** - Verify codegen → transform → runtime flow works correctly
7. **Cleanup and finalization** - Fix warnings, format code, ensure tests pass

## Success Criteria

- `LpLibFn` has methods to determine q32 mapping requirements
- Codegen emits TestCase calls for simplex functions (not hash functions)
- Generator uses `LpLibFn` enum as source of truth
- Registry has correct `BuiltinId` names (LpSimplex3, not Q32LpSimplex3)
- Transform correctly converts TestCase calls to q32 builtin calls
- All code compiles without warnings
- Tests pass verifying the conversion flow
