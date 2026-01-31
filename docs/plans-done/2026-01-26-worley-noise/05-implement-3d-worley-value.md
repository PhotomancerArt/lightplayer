# Phase 5: Implement 3D Worley Value

## Description

Create `worley3_value_q32.rs` implementing 3D Worley noise that returns the hash value of the
nearest cell.

## Implementation

### Files to Create

1. **`lp-glsl-builtins/src/builtins/lpfx/worley/worley3_value_q32.rs`**
    - `__lpfx_worley3_value_q32(x: i32, y: i32, z: i32, seed: u32) -> i32`
    - 3D Worley noise returning hash value

### Algorithm Reference

Reference implementation: `noise-rs/src/core/worley.rs` - `worley_3d` function with
`ReturnType::Value`

Key components:

- Same cell determination and feature point finding as 3D distance variant
- Instead of returning distance, return hash value of the seed_cell
- Hash value normalized to [0, 1] then scaled to [-1, 1]

### Implementation Details

1. Reuse the same algorithm as 3D distance variant to find nearest cell
2. Track which cell (`seed_cell`) contains the nearest feature point
3. Hash the `seed_cell` coordinates using `__lpfx_hash_3`
4. Normalize hash to [0, 1] range (divide by max hash value)
5. Scale to [-1, 1] range: `value * 2.0 - 1.0`
6. Return Q32 fixed-point value

## Success Criteria

- Function compiles without errors
- Function has `#[lpfx_impl_macro::lpfx_impl]` attribute with correct GLSL signature
- Function has `#[unsafe(no_mangle)]` attribute and `pub extern "C"` signature
- Function uses `__lpfx_hash_3` from hash module
- Output values are in approximately [-1, 1] range (Q32)
- Basic test verifies function produces different outputs for different inputs
- Test verifies value variant produces different output than distance variant
- Code formatted with `cargo +nightly fmt`

## Notes

- Can reuse helper functions from `worley3_q32.rs` if extracted to shared module
- Or implement independently following same pattern
- Hash normalization: `hash_value as f64 / 255.0` then scale to [-1, 1]
- The hash value comes from the cell coordinates, not the feature point
