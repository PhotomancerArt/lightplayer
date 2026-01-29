# Phase 6: Cleanup and Finalization

## Description

Final cleanup phase to ensure everything is correct, remove any temporary code, fix warnings, and verify the implementation is complete.

## Implementation Steps

1. Review all modified files for:
   - Unused imports
   - Dead code
   - TODO comments
   - Debug prints
   - Inconsistent formatting

2. Fix all compiler warnings (except unused code that will be used in future phases, if any)

3. Run full test suite:
   ```bash
   cargo test --workspace
   ```

4. Run filetests:
   ```bash
   scripts/glsl-filetests.sh
   ```

5. Verify code formatting:
   ```bash
   cargo +nightly fmt --check
   ```
   If needed, format:
   ```bash
   cargo +nightly fmt
   ```

6. Verify no old function names remain in:
   - Source code
   - Test files
   - Documentation
   - Generated code

7. Check that all overloads work correctly:
   - `lpfx_hash(uint)` works
   - `lpfx_hash(uvec2)` works
   - `lpfx_hash(uvec3)` works
   - `lpfx_snoise(float)` works
   - `lpfx_snoise(vec2)` works
   - `lpfx_snoise(vec3)` works
   - `lpfx_worley(vec2)` works
   - `lpfx_worley(vec3)` works
   - `lpfx_worley_value(vec2)` works
   - `lpfx_worley_value(vec3)` works

8. Move plan directory to `_done`:
   ```bash
   mv lp-glsl/plans/2026-01-26-lpfx-old-functions-overloads lp-glsl/plans/_done/
   ```

## Success Criteria

- All code compiles without warnings
- All tests pass
- All filetests pass
- Code is properly formatted
- No old function names remain
- All overloads work correctly
- Plan directory moved to `_done`

## Notes

- Take time to verify overload resolution is working correctly
- Make sure the public Rust function names match lygia's pattern
- Ensure the GLSL function names use overloads correctly
