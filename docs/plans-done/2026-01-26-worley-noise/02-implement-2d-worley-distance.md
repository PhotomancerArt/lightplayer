# Phase 2: Implement 2D Worley Distance

## Description

Create `worley2_q32.rs` implementing 2D Worley noise that returns the euclidean squared distance to
the nearest feature point.

## Implementation

### Files to Create

1. **`lp-glsl-builtins/src/builtins/lpfx/worley/worley2_q32.rs`**
    - `__lpfx_worley2_q32(x: i32, y: i32, seed: u32) -> i32`
    - 2D Worley noise returning distance

### Algorithm Reference

Reference implementation: `noise-rs/src/core/worley.rs` - `worley_2d` function

Key components:

- Cell determination (floor coordinates)
- Near/far cell selection based on fractional coordinates > 0.5
- Feature point generation using hash function (call `__lpfx_hash_2`)
- Distance calculation (euclidean squared - no sqrt)
- Range optimization (only check cells within distance range)
- Scaling to approximately [-1, 1] range (Q32 fixed-point)

### Q32 Fixed-Point Considerations

- All coordinates and return values are Q32 (i32 with 16.16 format)
- Use Q32 arithmetic operations (from `lp-glsl-builtins/src/util/q32/q32.rs`)
- Distance calculations use fixed-point arithmetic
- Final scaling factor accounts for Q32 format

### Implementation Details

1. Convert input coordinates to Q32
2. Calculate cell coordinates (floor)
3. Calculate fractional coordinates
4. Determine near/far cells based on fractional > 0.5
5. Generate feature point for near cell using hash
6. Calculate initial distance (euclidean squared)
7. Check adjacent cells only if within distance range
8. Scale distance to [-1, 1] range
9. Return Q32 fixed-point value

## Success Criteria

- Function compiles without errors
- Function has `#[lpfx_impl_macro::lpfx_impl]` attribute with correct GLSL signature
- Function has `#[unsafe(no_mangle)]` attribute and `pub extern "C"` signature
- Function uses `__lpfx_hash_2` from hash module
- Output values are in approximately [-1, 1] range (Q32)
- Basic test verifies function produces different outputs for different inputs
- Code formatted with `cargo +nightly fmt`

## Notes

- Place helper utility functions at the bottom of files
- Reference noise-rs implementation but adapt for Q32 fixed-point
- Use existing Q32 utilities from `lp-glsl-builtins/src/util/q32/q32.rs`
- Include comments explaining Worley noise algorithm steps
- The `get_vec2` function from reference implementation generates feature point offsets
