# Phase 6: Review and Enhance Tests

## Description

Review existing out/inout parameter tests and add additional test cases for edge cases and array
support.

## Implementation

### Files to Review

1. **`lp-glsl/lp-glsl-filetests/filetests/function/param-out.glsl`**
2. **`lp-glsl/lp-glsl-filetests/filetests/function/param-inout.glsl`**
3. **`lp-glsl/lp-glsl-filetests/filetests/function/param-mixed.glsl`**
4. **`lp-glsl/lp-glsl-filetests/filetests/function/edge-lvalue-out.glsl`**

### Test Cases to Add

1. **Array out/inout parameters** (NEW)
    - `void set_array_element(out float[3] arr, int idx)`
    - Test passing array elements as out/inout
    - Test passing entire array as out/inout

2. **Multiple out parameters**
    - Verify copy-back order (left to right)
    - Test with different types (scalar, vector, matrix)

3. **Out parameter aliasing**
    - Pass same variable twice as out parameters
    - Verify both get updated correctly

4. **Vector/matrix out parameters**
    - Test component access (e.g., `result.x`) in function body
    - Test full vector/matrix assignment

5. **LPFX function out parameters** (if applicable)
    - Test native function with out parameter
    - Verify pointer passing works correctly

### Success Criteria

- All existing tests pass
- New test cases are added and pass
- Tests cover edge cases (aliasing, multiple outs, arrays)
- No test failures

## Notes

- Skip struct tests (no struct support yet)
- Focus on array support for out/inout parameters
- Ensure tests are clear and well-documented
- Test both success cases and error cases (lvalue validation)
