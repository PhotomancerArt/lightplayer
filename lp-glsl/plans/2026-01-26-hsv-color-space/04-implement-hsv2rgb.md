# Phase 4: Implement hsv2rgb_q32 Conversion Function

## Description

Implement the `hsv2rgb` function which converts HSV color space to RGB. This function uses `hue2rgb` and `saturate` helpers.

## Implementation

### File: `lpfx/color/space/hsv2rgb_q32.rs`

Implement following the two-layer pattern:

**Public Rust functions:**
- `lpfx_hsv2rgb_q32(hsv: Vec3Q32) -> Vec3Q32` - Convert HSV to RGB
  - Algorithm: `((lpfx_hue2rgb_q32(hsv.x) - Vec3Q32::one()) * hsv.y + Vec3Q32::one()) * hsv.z`
  - Uses `lpfx_hue2rgb_q32` (calls the public Rust function)
- `lpfx_hsv2rgb_vec4_q32(hsv: Vec4Q32) -> Vec4Q32` - Convert HSV to RGB (preserves alpha)
  - Applies `lpfx_hsv2rgb_q32` to RGB components, preserves alpha

**Extern C wrappers:**
- `__lpfx_hsv2rgb_q32(x: i32, y: i32, z: i32) -> (i32, i32, i32)` - Wraps `lpfx_hsv2rgb_q32`
- `__lpfx_hsv2rgb_vec4_q32(x: i32, y: i32, z: i32, w: i32) -> (i32, i32, i32, i32)` - Wraps `lpfx_hsv2rgb_vec4_q32`
  - Both have `#[lpfx_impl_macro::lpfx_impl]` annotations with GLSL signatures
  - Both have `#[unsafe(no_mangle)]` and `pub extern "C"` attributes
  - Convert expanded types to nice types, call the `lpfx_*` function, convert back

### Update Module

Update `lpfx/color/space/mod.rs` to export `hsv2rgb_q32`.

## Success Criteria

- hsv2rgb_q32 functions implemented for Vec3Q32 and Vec4Q32
- Uses hue2rgb_q32 and saturate_vec3_q32 internally
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
