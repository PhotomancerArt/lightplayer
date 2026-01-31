# Phase 2: Implement Simplex Noise Functions

## Description

Create Simplex noise implementations in three files: `lpfx_snoise1.rs`, `lpfx_snoise2.rs`, and
`lpfx_snoise3.rs`. These implement 1D, 2D, and 3D Simplex noise using Q32 fixed-point arithmetic.

## Implementation

### Files to Create

1. **`lp-glsl-builtins/src/builtins/q32/lpfx_snoise1.rs`**
    - `__lpfx_snoise1(x: i32, seed: u32) -> i32`
    - 1D Simplex noise

2. **`lp-glsl-builtins/src/builtins/q32/lpfx_snoise2.rs`**
    - `__lpfx_snoise2(x: i32, y: i32, seed: u32) -> i32`
    - 2D Simplex noise

3. **`lp-glsl-builtins/src/builtins/q32/lpfx_snoise3.rs`**
    - `__lpfx_snoise3(x: i32, y: i32, z: i32, seed: u32) -> i32`
    - 3D Simplex noise

### Algorithm Reference

Reference implementation: `noise-rs/src/core/simplex.rs`

Key components:

- Skew/unskew factors for coordinate transformation
- Simplex cell determination
- Gradient selection using hash function (call `__lpfx_hash_*` functions)
- Smooth interpolation using quintic curve
- Scaling to approximately [-1, 1] range (Q32 fixed-point)

### Q32 Fixed-Point Considerations

- All coordinates and return values are Q32 (i32 with 16.16 format)
- Use Q32 arithmetic operations (from `lp-glsl-builtins/src/q32/q32.rs`)
- Skew/unskew calculations need to work with fixed-point
- Interpolation uses fixed-point arithmetic
- Final scaling factor accounts for Q32 format

## Success Criteria

- All three Simplex functions compile
- Functions have `#[unsafe(no_mangle)]` attribute and `pub extern "C"` signature
- Functions use hash functions from phase 1
- Output values are in approximately [-1, 1] range (Q32)
- Basic tests verify noise produces different outputs for different inputs
- Code formatted with `cargo +nightly fmt`

## Notes

- Place helper utility functions at the bottom of files
- Reference noise-rs implementation but adapt for Q32 fixed-point
- Use existing Q32 utilities from `lp-glsl-builtins/src/q32/q32.rs`
- Include comments explaining Simplex noise algorithm steps
