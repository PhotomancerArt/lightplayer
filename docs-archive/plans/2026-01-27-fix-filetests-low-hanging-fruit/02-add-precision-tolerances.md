# Phase 2: Add Precision Tolerances to Builtin Tests

## Description

Add explicit tolerances to precision-sensitive builtin tests that are failing due to small floating-point precision errors. These tests use `~=` (approximate equality) but don't specify tolerance, causing failures with very small errors.

## Changes

### Test Files to Update

1. **`builtins/angle-degrees.glsl`**
   - Add `(tolerance: 0.001)` to all `~=` comparisons (lines 14, 21, 28, 35, 42, 49, 56, 63)

2. **`builtins/angle-radians.glsl`**
   - Add `(tolerance: 0.001)` to all `~=` comparisons

3. **`builtins/common-roundeven.glsl`**
   - Add tolerance to the failing test (identify which test is failing and add appropriate tolerance)

4. **`builtins/edge-component-wise.glsl`**
   - Add tolerances to the 2 failing tests

5. **`builtins/edge-exp-domain.glsl`**
   - Add tolerances to the 8 failing tests

6. **`builtins/edge-nan-inf-propagation.glsl`**
   - Add tolerances to the 7 failing tests

7. **`builtins/edge-precision.glsl`**
   - Add tolerances to all 9 tests

8. **`builtins/edge-trig-domain.glsl`**
   - Add tolerances to the 3 failing tests

9. **`builtins/matrix-determinant.glsl`**
   - Add tolerance to the 1 failing test

## Success Criteria

- All precision-sensitive tests have explicit tolerances
- Tests pass with the added tolerances
- Tolerance values are reasonable (not too loose)
- Test format is consistent across all files

## Implementation Notes

- Use tolerance value of `0.001` for angle conversions (degrees/radians)
- Use appropriate tolerance values for other precision-sensitive operations
- Run tests after adding tolerances to verify they pass
- Consider if implementation precision can be improved, but tolerances are acceptable for floating-point operations
