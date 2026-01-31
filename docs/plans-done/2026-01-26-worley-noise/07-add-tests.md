# Phase 7: Add Tests

## Description

Create comprehensive tests for Worley noise functions, comparing against the noise-rs reference
implementation and testing basic properties.

## Implementation

### Test Files

Add tests to each implementation file in `#[cfg(test)]` modules:

- `worley2_q32.rs` - tests for 2D distance
- `worley2_value_q32.rs` - tests for 2D value
- `worley3_q32.rs` - tests for 3D distance
- `worley3_value_q32.rs` - tests for 3D value

### Test Cases

1. **Basic functionality tests**
    - Different inputs produce different outputs
    - Same input and seed produce same output (deterministic)
    - Different seeds produce different outputs

2. **Range tests**
    - Output values are approximately in [-1, 1] range
    - Test with various input coordinates

3. **Comparison with reference implementation**
    - Convert Q32 outputs to f64
    - Compare against noise-rs `worley_2d` / `worley_3d` outputs
    - Allow for small differences due to fixed-point precision

4. **Distance vs Value tests**
    - Verify that distance and value variants produce different outputs
    - Verify that value variant is deterministic based on cell

### Test Dependencies

- Add `noise` crate as dev dependency if not already present
- Use existing test helpers from `lp-glsl-builtins/src/util/test_helpers.rs`

## Success Criteria

- All tests pass
- Tests verify basic functionality (determinism, range, etc.)
- Tests compare against noise-rs reference implementation
- Tests verify distance and value variants behave correctly
- Code formatted with `cargo +nightly fmt`

## Notes

- Use `fixed_to_float` and `float_to_fixed` helpers for conversions
- Allow for small differences in comparison tests due to fixed-point precision
- Test with various seed values and coordinate ranges
- Place test utility functions at the bottom of test modules
