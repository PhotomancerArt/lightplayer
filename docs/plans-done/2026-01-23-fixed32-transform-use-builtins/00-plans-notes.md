# Questions for Q32 Transform Use Builtins Plan

## Current State

The q32 transform currently generates inline saturation code for arithmetic operations:

- `fadd`: Generates ~20 instructions with inline saturation checks
- `fsub`: Generates ~20 instructions with inline saturation checks
- `fdiv`: Generates ~30 instructions with inline division logic (handles edge cases like small
  divisors < 2^16)
- `fmul`: Already uses builtin `__lp_q32_mul` ✅

The `__lp_q32_div` builtin exists but is intentionally NOT used by the transform because:

- The inline code handles edge cases that the builtin may not handle correctly
- Code comment mentions "bug fix for small divisors < 2^16"
- Test for `fdiv` is currently ignored due to "known issue with the division algorithm"

## Goal

Update the q32 transform to use builtins for `add`, `sub`, and `div` operations, following the same
pattern as `mul`. This will reduce code bloat from ~20-30 instructions per operation to a single
function call.

## Questions

1. **Division Builtin Edge Cases**: ✅ **ANSWERED** - Option B: Verify that `__lp_q32_div` already
   handles edge cases correctly, then use it.
    - We absolutely want to use the builtin - that's why it exists
    - Need to verify it handles small divisors (< 2^16) correctly
    - If verification shows issues, we'll fix the builtin to match inline code behavior

2. **Add/Sub Builtin Implementation**: ✅ **ANSWERED** - Use i64 operations for speed, then clamp to
   min/max.
    - Use 64-bit intermediate calculations to avoid overflow (like `mul` does)
    - Perform addition/subtraction in i64, then clamp result to [MIN_FIXED, MAX_FIXED]
    - This is simpler and faster than the current inline code's multiple sign checks
    - Must ensure clamping to min/max for correctness

3. **Testing Strategy**: ✅ **ANSWERED** - Rely on filetests for full testing.
    - Unit tests in arithmetic.rs are just sanity tests
    - Add and sub tests already pass fully
    - Div has one bug - hopefully using the builtin will fix it
    - Unignore the `test_q32_fdiv` test - it should work now with the builtin
    - Filetests provide comprehensive coverage

4. **Code Size Verification**: ✅ **ANSWERED** - Run lp-glsl-q32-metrics-app script after changes to
   compare with before.
    - Before state already captured in `docs/reports/q32/2026-01-24T01.26.02-pre-ops-builtins`
    - Run script again after implementation to compare and verify code size reduction
