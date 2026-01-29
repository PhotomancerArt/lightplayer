# Phase 7: Cleanup and Finalization

## Description

Final cleanup phase: remove temporary code, fix warnings, ensure all tests pass, and format code.

## Implementation

### Tasks

1. **Remove Temporary Code**
   - Remove any debug prints or temporary workarounds
   - Remove TODO comments that are no longer relevant
   - Clean up any unused helper functions

2. **Fix Warnings**
   - Run `cargo +nightly fmt` on all modified files
   - Fix any compiler warnings
   - Ensure no unused code (except if intentionally left for future phases)

3. **Verify Tests**
   - Run all filetests: `just test-filetests` or equivalent
   - Verify all out/inout parameter tests pass
   - Verify no regressions in other tests

4. **Code Review**
   - Ensure code follows project conventions
   - Verify error messages are clear
   - Check that code is well-documented

5. **Final Formatting**
   - Run `cargo +nightly fmt` on entire workspace
   - Ensure consistent formatting across all modified files

### Success Criteria

- All code compiles without warnings
- All tests pass
- Code is formatted with `cargo +nightly fmt`
- No temporary code or TODOs remain
- Code follows project conventions

## Notes

- This is the final phase before marking plan as complete
- Move plan directory to `lp-glsl/plans/_done/` after completion
- Commit with message: `lpc: out-parameters - complete plan`
