# Phase 3: Implement hue2rgb_q32 Helper Function

## Description

Implement the `hue2rgb` function which converts a hue value (0-1) to an RGB vec3. This is a helper function used by `hsv2rgb`.

## Implementation

### File: `lpfx/color/space/hue2rgb_q32.rs`

Implement following the two-layer pattern:

**Public Rust function:**
- `lpfx_hue2rgb_q32(hue: Q32) -> Vec3Q32` - Convert hue value to RGB
  - Algorithm: Uses abs() and arithmetic operations to compute RGB from hue
  - Formula: `R = abs(hue * 6.0 - 3.0) - 1.0`, `G = 2.0 - abs(hue * 6.0 - 2.0)`, `B = 2.0 - abs(hue * 6.0 - 4.0)`
  - Result should be saturated using `lpfx_saturate_vec3_q32` (calls the public Rust function)

**Extern C wrapper:**
- `__lpfx_hue2rgb_q32(hue: i32) -> (i32, i32, i32)` - Wraps `lpfx_hue2rgb_q32`
  - `#[lpfx_impl_macro::lpfx_impl]` annotation with GLSL signature
  - `#[unsafe(no_mangle)]` and `pub extern "C"` attributes
  - Converts i32 to Q32, calls `lpfx_hue2rgb_q32`, converts Vec3Q32 result back to three i32s

### Update Module

Update `lpfx/color/space/mod.rs` to export `hue2rgb_q32`.

## Success Criteria

- hue2rgb_q32 function implemented
- Uses Q32 arithmetic and Vec3Q32
- Calls saturate_vec3_q32 to clamp result
- Function follows the inlinable implementation + extern C wrapper pattern
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
