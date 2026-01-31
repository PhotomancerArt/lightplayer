# Phase 1: Implement Hash Function

## Description

Create the hash function implementation in `lp-glsl-builtins/src/builtins/q32/lpfx_hash.rs` with
three variants for 1D, 2D, and 3D hashing. The hash function uses the noiz algorithm optimized for
noise generation.

## Implementation

### File: `lp-glsl-builtins/src/builtins/q32/lpfx_hash.rs`

Implement three hash functions:

- `__lpfx_hash_1(x: u32, seed: u32) -> u32` - 1D hash
- `__lpfx_hash_2(x: u32, y: u32, seed: u32) -> u32` - 2D hash
- `__lpfx_hash_3(x: u32, y: u32, z: u32, seed: u32) -> u32` - 3D hash

Use the noiz hash algorithm:

- Large prime: 249,222,277 (KEY)
- Bit rotations and XOR operations
- Include attribution comment referencing noiz library and nullprogram blog post

### Algorithm Reference

Based on noiz's `rand_u32` implementation:

```rust
const KEY: u32 = 249_222_277;
let mut x = input;
x ^= x.rotate_right(17);
x = x.wrapping_mul(KEY);
x ^= x.rotate_right(11) ^ seed;
x = x.wrapping_mul(!KEY);
x
```

For multi-dimensional inputs, combine coordinates non-commutatively before hashing (similar to
noiz's `NoiseRngInput` trait implementations).

## Success Criteria

- All three hash functions compile
- Functions have `#[unsafe(no_mangle)]` attribute and `pub extern "C"` signature
- Hash algorithm matches noiz implementation with attribution
- Basic tests verify hash produces different outputs for different inputs
- Code formatted with `cargo +nightly fmt`

## Notes

- Place helper utility functions at the bottom of the file
- Include attribution comment for noiz algorithm
- Use `#[cfg(test)]` for test module
