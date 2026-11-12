# Phase 7: Cleanup and verification

## Description

Final cleanup phase: remove any temporary code, ensure all tests pass, fix warnings, and verify the refactoring is complete.

## Implementation

1. **Remove temporary code:**
   - Remove any debug prints or temporary variables
   - Remove any TODOs or comments that are no longer relevant
   - Clean up any unused imports

2. **Fix warnings:**
   - Address all compiler warnings
   - Ensure no unused code (except that which will be used in future phases)
   - Fix any clippy warnings

3. **Verify tests:**
   - Run all existing tests for psrdnoise2_q32 and psrdnoise3_q32
   - Verify all tests pass (no functional changes expected)
   - Add any missing tests for edge cases

4. **Code review:**
   - Verify code matches GLSL structure more closely
   - Check that vector operations are used consistently
   - Ensure helper functions are used appropriately

5. **Formatting:**
   - Run `cargo +nightly fmt` on entire workspace
   - Ensure consistent formatting across all modified files

6. **Documentation:**
   - Update any relevant documentation
   - Ensure function comments are accurate

## Success Criteria

- No temporary code or TODOs remain
- All warnings fixed
- All tests pass
- Code is clean and readable
- Code formatted with `cargo +nightly fmt`
- Ready to move plan to `_done/`

## Code Organization

- Place helper utility functions at the bottom of files
- Place more abstract things, entry points, and tests first
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete solution"
- Avoid emoticons
- Code is never done, never perfect, never fully ready, never fully complete
- Use measured, factual descriptions of what was implemented
