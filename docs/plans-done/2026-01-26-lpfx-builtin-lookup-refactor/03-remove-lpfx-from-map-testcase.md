# Phase 3: Remove LPFX Entries from `map_testcase_to_builtin`

## Description

Update the codegen tool to stop generating LPFX function entries in `map_testcase_to_builtin`. LPFX
functions are now handled via the proper lookup chain.

## Implementation

1. Update `lp-glsl/lp-glsl-builtin-gen-app/src/main.rs`:
    - In `generate_map_testcase_to_builtin`, filter out LPFX functions
    - Only generate entries for regular q32 functions (e.g., `__lp_q32_sin`)
    - LPFX functions should be skipped entirely

2. The filtering logic should:
    - Check if function is LPFX (e.g., check module path or symbol name pattern)
    - Skip LPFX functions when generating match arms
    - Keep regular q32 functions unchanged

## Success Criteria

- Codegen tool no longer generates LPFX entries in `map_testcase_to_builtin`
- Regular q32 functions still have entries
- Generated code compiles without errors
- No LPFX function names appear in `map_testcase_to_builtin` match statement
