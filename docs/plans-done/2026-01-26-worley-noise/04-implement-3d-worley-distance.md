# Phase 4: Implement 3D Worley Distance

## Description

Create `worley3_q32.rs` implementing 3D Worley noise that returns the euclidean squared distance to
the nearest feature point.

## Implementation

### Files to Create

1. **`lp-glsl-builtins/src/builtins/lpfx/worley/worley3_q32.rs`**
    - `__lpfx_worley3_q32(x: i32, y: i32, z: i32, seed: u32) -> i32`
    - 3D Worley noise returning distance

### Algorithm Reference

Reference implementation: `noise-rs/src/core/worley.rs` - `worley_3d` function

Key components:

- Same as 2D but extended to 3D
- Cell determination (floor coordinates in 3D)
- Near/far cell selection based on fractional coordinates > 0.5
- Feature point generation using hash function (call `__lpfx_hash_3`)
- Distance calculation (euclidean squared - no sqrt)
- Range optimization (check up to 7 adjacent cells within distance range)
- Scaling to approximately [-1, 1] range (Q32 fixed-point)

### Implementation Details

1. Convert input coordinates to Q32
2. Calculate cell coordinates (floor in 3D)
3. Calculate fractional coordinates
4. Determine near/far cells based on fractional > 0.5 (for x, y, z)
5. Generate feature point for near cell using hash
6. Calculate initial distance (euclidean squared)
7. Check adjacent cells (up to 7) only if within distance range
8. Scale distance to [-1, 1] range
9. Return Q32 fixed-point value

## Success Criteria

- Function compiles without errors
- Function has `#[lpfx_impl_macro::lpfx_impl]` attribute with correct GLSL signature
- Function has `#[unsafe(no_mangle)]` attribute and `pub extern "C"` signature
- Function uses `__lpfx_hash_3` from hash module
- Output values are in approximately [-1, 1] range (Q32)
- Basic test verifies function produces different outputs for different inputs
- Code formatted with `cargo +nightly fmt`

## Notes

- Place helper utility functions at the bottom of files
- Reference noise-rs implementation but adapt for Q32 fixed-point
- Use existing Q32 utilities from `lp-glsl-builtins/src/util/q32/q32.rs`
- Include comments explaining Worley noise algorithm steps
- The `get_vec3` function from reference implementation generates feature point offsets
- 3D version checks more adjacent cells (up to 7) compared to 2D (up to 3)
