# Phase 5: Implement rgb2hsv_q32 Conversion Function

## Description

Implement the `rgb2hsv` function which converts RGB color space to HSV. This function uses an epsilon constant to avoid division by zero.

## Implementation

### File: `lpfx/color/space/rgb2hsv_q32.rs`

Implement following the two-layer pattern:

**Public Rust functions:**
- `lpfx_rgb2hsv_q32(rgb: Vec3Q32) -> Vec3Q32` - Convert RGB to HSV
  - Algorithm from lygia (Sam Hocevar's implementation)
  - Uses Vec4Q32 K with constants: `Vec4Q32::new(0., -0.33333333333333333333, 0.6666666666666666666, -1.0)`
  - Computes p and q based on component comparisons
  - Uses epsilon to avoid division by zero: `d / (q.x + HCV_EPSILON_Q32)`
  - Returns HSV as `Vec3Q32(hue, saturation, value)`
- `lpfx_rgb2hsv_vec4_q32(rgb: Vec4Q32) -> Vec4Q32` - Convert RGB to HSV (preserves alpha)
  - Applies `lpfx_rgb2hsv_q32` to RGB components, preserves alpha

For Q32 epsilon:
- Use minimum representable Q32 value or very small representable value
- Define as constant: `const HCV_EPSILON_Q32: Q32 = ...`

**Extern C wrappers:**
- `__lpfx_rgb2hsv_q32(x: i32, y: i32, z: i32) -> (i32, i32, i32)` - Wraps `lpfx_rgb2hsv_q32`
- `__lpfx_rgb2hsv_vec4_q32(x: i32, y: i32, z: i32, w: i32) -> (i32, i32, i32, i32)` - Wraps `lpfx_rgb2hsv_vec4_q32`
  - Both have `#[lpfx_impl_macro::lpfx_impl]` annotations with GLSL signatures
  - Both have `#[unsafe(no_mangle)]` and `pub extern "C"` attributes
  - Convert expanded types to nice types, call the `lpfx_*` function, convert back

### Update Module

Update `lpfx/color/space/mod.rs` to export `rgb2hsv_q32`.

## Success Criteria

- rgb2hsv_q32 functions implemented for Vec3Q32 and Vec4Q32
- Epsilon constant properly defined for Q32
- Functions follow the inlinable implementation + extern C wrapper pattern
- Code compiles
- Code formatted with `cargo +nightly fmt`

## Style Notes

### Code Organization

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Keep related functionality grouped together

### Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

### Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete solution"
- Avoid emoticons
- Code is never done, never perfect, never fully ready, never fully complete
- Use measured, factual descriptions of what was implemented
