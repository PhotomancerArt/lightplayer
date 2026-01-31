# Phase 4: Add Unit Tests for Lookup Chain

## Description

Add comprehensive unit tests to verify the lookup chain works correctly:

- `builtin_id_from_name` reverse lookup
- Full chain: `name` → `BuiltinId` → `LpfxFn` → `q32_impl` → `name`

## Implementation

1. Add tests to `lp-glsl/lp-glsl-compiler/src/backend/builtins/registry.rs`:
    - Test `builtin_id_from_name` for various builtin names
    - Test round-trip: `builtin_id.name()` → `builtin_id_from_name()` → same `builtin_id`
    - Test unknown names return `None`

2. Add tests to `lp-glsl/lp-glsl-compiler/src/frontend/semantic/lpfx/lpfx_fn_registry.rs`:
    - Test full lookup chain for LPFX functions (f32 → q32)
    - Test that `find_lpfx_fn_by_builtin_id` correctly finds LPFX functions
    - Test that non-LPFX builtins return `None` from `find_lpfx_fn_by_builtin_id`

## Success Criteria

- Unit tests added for `builtin_id_from_name`
- Unit tests added for full lookup chain
- All tests pass
- Tests cover edge cases (unknown names, non-LPFX builtins, etc.)
