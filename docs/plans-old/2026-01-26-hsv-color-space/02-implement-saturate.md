# Phase 2: Implement saturate_q32 Math Utility

## Description

Implement the `saturate` function which clamps values between 0 and 1. This is a simple utility function used by color space conversions.

## Implementation

### File: `lpfx/math/saturate_q32.rs`

Implement following the two-layer pattern:

**Public Rust functions (can be inlined):**
- `lpfx_saturate_q32(value: Q32) -> Q32` - Clamp single Q32 value to [0, 1]
- `lpfx_saturate_vec3_q32(v: Vec3Q32) -> Vec3Q32` - Saturate each component of vec3
- `lpfx_saturate_vec4_q32(v: Vec4Q32) -> Vec4Q32` - Saturate each component of vec4

**Extern C wrappers (for compiler):**
- `__lpfx_saturate_q32(value: i32) -> i32` - Wraps `lpfx_saturate_q32`
- `__lpfx_saturate_vec3_q32(x: i32, y: i32, z: i32) -> (i32, i32, i32)` - Wraps `lpfx_saturate_vec3_q32`
- `__lpfx_saturate_vec4_q32(x: i32, y: i32, z: i32, w: i32) -> (i32, i32, i32, i32)` - Wraps `lpfx_saturate_vec4_q32`

Each extern C wrapper should have:
- `#[lpfx_impl_macro::lpfx_impl]` annotation with appropriate GLSL signature
- `#[unsafe(no_mangle)]` and `pub extern "C"` attributes
- Convert expanded types to nice types, call the `lpfx_*` function, convert back

### Update Module

Update `lpfx/math/mod.rs` to export `saturate_q32`.

## Success Criteria

- Saturate functions implemented for Q32, Vec3Q32, and Vec4Q32
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
