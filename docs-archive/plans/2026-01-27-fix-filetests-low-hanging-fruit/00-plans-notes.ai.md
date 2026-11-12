# Plan: Fix Filetests Low Hanging Fruit & Test Framework Bugs

## Overview

Fix the high-priority bugs identified in the filetests failure analysis that prevent tests from running correctly. Focus on test infrastructure bugs and compiler bugs that affect multiple tests.

## Scope

Based on the analysis report, we'll focus on:

1. **Control Flow Variable Scoping** - Bugs in for loop and if block variable shadowing
2. **Builtin Precision Issues** - Add tolerances to precision-sensitive tests
3. **Vec Comparison Functions** - Bug in `equal()` with bvec2 arguments
4. **Vec Conversion** - Bug in `uint()` cast with negative values

**Note:** Matrix Struct Return ABI issue is handled in a separate plan: `2026-01-27-fix-matrix-structreturn-test-runner`

## Questions

### Q1: Control Flow Variable Scoping - Test Expectations

**Question:** What is the correct behavior for variable shadowing in for loops and if blocks?

**Context:**

- `control/for/variable-scope.glsl` test `test_for_loop_init_shadowing()` expects outer `i` to be 3, but comment says "Outer i should be unchanged" (should be 100)
- `control/if/variable-scope.glsl` test `test_if_variable_shadowing()` expects outer `x` to be 10, but comment says "Inner x shadows outer x" (should be 5)
- There's a contradiction between test expectations and comments

**Suggested Answer:**

- In GLSL, variable shadowing should work: inner scope variables shadow outer scope variables
- For `test_for_loop_init_shadowing()`: The outer `i` should remain 100 (shadowed by loop variable). The test expectation of 3 is wrong - it should expect 100.
- For `test_if_variable_shadowing()`: The outer `x` should remain 5 (shadowed by inner `x`). The test expectation of 10 is wrong - it should expect 5.
- We should fix the tests to match GLSL semantics, then fix the compiler to implement correct shadowing.

**Decision:** Fix test expectations first to match GLSL semantics, then fix compiler shadowing implementation.

**Answer:** Fix expectations to match GLSL - outer variables should remain unchanged when shadowed by inner scope variables.

---

### Q2: Builtin Precision Issues - Tolerance Strategy

**Question:** Should we add tolerances to tests or fix the precision in implementations?

**Context:**

- Tests like `angle-degrees.glsl` fail with small precision errors (e.g., 90.000244 vs 90.0)
- The tests use `~=` (approximate equality) but don't specify tolerance
- Default tolerance might be too strict for these operations

**Suggested Answer:**

- For trigonometric and angle conversion functions, small precision errors are expected due to floating-point arithmetic
- We should add explicit tolerances to the tests (e.g., `~= 90.0 (tolerance: 0.001)`)
- This is more appropriate than trying to achieve perfect precision, which may not be possible
- However, we should also verify the implementations are correct (not introducing unnecessary errors)

**Decision:** Add tolerances to precision-sensitive tests, verify implementations are reasonable.

**Answer:** Add explicit tolerances to precision-sensitive tests (e.g., `~= 90.0 (tolerance: 0.001)`).

---

### Q3: Vec Comparison Functions - equal() with bvec2

**Question:** How should `equal()` handle bvec2 arguments in nested calls?

**Context:**

- `vec/vec2/fn-equal.gen.glsl` test `test_vec2_equal_function_in_expression()` fails
- Test calls `equal(equal(a, b), equal(b, c))` where `equal(a, b)` returns `bvec2(true, false)`
- Expected: `bvec2(false, false)`, Actual: `bvec2(false, true)`
- This suggests `equal()` doesn't correctly handle bvec2 arguments

**Investigation needed:**

- Check how `equal()` is implemented for bvec2 arguments
- Verify if there's a type mismatch or incorrect comparison logic

**Suggested Answer:**

- `equal()` should work with bvec2 arguments (comparing boolean vectors component-wise)
- The bug is likely in the implementation - either type handling or comparison logic
- We need to fix the `equal()` builtin function to handle bvec2 correctly

**Decision:** Fix the `equal()` function implementation to handle bvec2 arguments correctly.

**Answer:** Investigate and fix the `equal()` function implementation to handle bvec2 arguments correctly.

---

### Q4: Vec Conversion - uint() Cast with Negative Values

**Question:** How should `uint()` cast handle negative float values?

**Context:**

- `vec/uvec2/from-scalars.glsl` test expects `uint(-3.2)` to wrap to `4294967293u`
- Actual result is `0u` (clamped to 0 instead of wrapped)

**Suggested Answer:**

- According to GLSL spec, converting negative floats to uint should wrap (modulo 2^32)
- The current implementation is clamping to 0, which is incorrect
- We need to fix the `uint()` cast implementation to wrap negative values

**Decision:** Fix the `uint()` cast implementation to wrap negative values according to GLSL spec.

**Answer:** Fix the `uint()` cast implementation to wrap negative values (modulo 2^32) instead of clamping to 0.

---

### Q5: Test File Review

**Question:** Should we review and improve existing filetests as part of this plan?

**Context:**

- User mentioned reviewing filetests (referenced terminal output)
- Some tests have contradictory expectations vs comments
- We should ensure tests are clear and correct

**Suggested Answer:**

- Yes, we should review and fix test files as we encounter issues
- Fix contradictory test expectations (as in Q2)
- Ensure tests are clear and follow GLSL semantics
- Add tolerances where needed (as in Q3)

**Decision:** Review and fix test files as part of fixing the bugs.

**Answer:** Fix test files as issues are encountered - fix contradictory expectations, ensure clarity, and add tolerances where needed.

---

## Notes

- Focus on high-priority bugs that affect many tests
- Variable scoping bugs affect control flow tests
- Precision and comparison bugs affect multiple vec/builtin tests
- These fixes will unblock many failing tests
- Matrix StructReturn handled separately
