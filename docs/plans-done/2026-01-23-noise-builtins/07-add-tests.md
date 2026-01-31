# Phase 7: Add Tests

## Description

Create tests comparing LP library function outputs against noise-rs reference implementation. Add
noise-rs as a test-only dependency and create comparison tests.

## Implementation

### Add Test Dependency

Update `lp-glsl-builtins/Cargo.toml`:

```toml
[dev-dependencies]
noise = "0.9"
```

### Test Files

Add tests to each implementation file:

1. **`lpfx_hash.rs` tests**
    - Test hash produces different outputs for different inputs
    - Test hash is deterministic (same input + seed = same output)
    - Test seed affects output

2. **`lpfx_snoise1.rs` tests**
    - Compare against noise-rs Simplex 1D
    - Test output range (approximately [-1, 1])
    - Test continuity (small input changes produce small output changes)

3. **`lpfx_snoise2.rs` tests**
    - Compare against noise-rs Simplex 2D
    - Test output range
    - Test continuity

4. **`lpfx_snoise3.rs` tests**
    - Compare against noise-rs Simplex 3D
    - Test output range
    - Test continuity

### Comparison Strategy

For each test:

1. Convert Q32 fixed-point input to f64
2. Call noise-rs function with f64 input
3. Convert noise-rs f64 output to Q32
4. Call LP library function with Q32 input
5. Compare outputs (allow small tolerance for fixed-point precision)

### Test Helpers

Create helper functions in test modules:

- `q32_to_f64(i32) -> f64` - Convert Q32 to f64
- `f64_to_q32(f64) -> i32` - Convert f64 to Q32
- `compare_noise_outputs()` - Compare with tolerance

## Success Criteria

- All tests compile
- Tests pass comparing against noise-rs
- Output ranges are validated
- Continuity is validated
- Seed determinism is validated
- Code formatted with `cargo +nightly fmt`

## Notes

- Use reasonable tolerance for fixed-point comparisons (e.g., 0.01)
- Test multiple input values across the range
- Test edge cases (zero, large values, negative values)
- Place test helpers at the bottom of test modules
