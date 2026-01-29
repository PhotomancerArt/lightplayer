# Phase 5: Regenerate Builtins and Verify All Tests Pass

## Description

Regenerate all builtin code using the updated codegen tool, then verify all tests pass.

## Implementation

1. Run `scripts/build-builtins.sh` to regenerate:
   - `registry.rs` with `builtin_id_from_name`
   - `math.rs` without LPFX entries in `map_testcase_to_builtin`

2. Run `just fix ci` to ensure code compiles and lints pass

3. Run `scripts/glsl-filetests.sh lpfx/lp_` to verify LPFX filetests pass

4. Run all unit tests to ensure nothing broke

## Success Criteria

- Builtins regenerate successfully
- All code compiles without errors
- All lints pass
- All filetests pass
- All unit tests pass
- No regressions introduced
