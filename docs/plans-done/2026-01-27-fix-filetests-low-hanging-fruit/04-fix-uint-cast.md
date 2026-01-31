# Phase 4: Fix uint() Cast for Negative Values

## Description

Fix the `uint()` cast function to wrap negative float values according to GLSL spec (modulo 2^32)
instead of clamping them to 0.

## Changes

### `lp-glsl/lp-glsl-compiler/src/frontend/codegen/expr/coercion.rs`

- **`float_to_uint()` conversion** (around line 98-101):
    - Current: Uses `fcvt_to_uint` which may clamp negatives
    - Fix: Wrap negative values using modulo 2^32
    - Implementation: Convert to i32 first, then cast to u32 (wraps automatically)

### `lp-glsl/lp-glsl-compiler/src/backend/transform/q32/converters/conversions.rs`

- **`convert_fcvt_to_uint()` function** (around line 189-236):
    - Current: Clamps negative values to 0 (line 222: `select(is_negative, zero, shifted)`)
    - Fix: Wrap negative values instead of clamping
    - Implementation:
        - For negative values, convert to i32, then cast to u32 (wraps)
        - Remove clamping logic, use wrapping conversion

## Success Criteria

- `uint(-3.2)` wraps to `4294967293u` instead of `0u`
- Test `test_uvec2_from_scalars_function_results()` passes
- All uvec2/3/4 from-scalars tests pass
- No regressions in other uint conversion tests
- Positive values still work correctly

## Implementation Notes

- GLSL spec says converting negative float to uint wraps (modulo 2^32)
- In Rust/Cranelift, casting negative i32 to u32 automatically wraps
- Need to ensure both frontend coercion and q32 backend conversion wrap correctly
- Test with various negative values to verify wrapping behavior
