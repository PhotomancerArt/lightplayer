# Phase 1: Add mix/lerp functions to Q32 and vector types

## Description

Add `mix()` (linear interpolation) functions to Q32 and all vector types (Vec2Q32, Vec3Q32, Vec4Q32). This function is needed by gnoise for interpolating between grid cell corners.

## Implementation

### File: `glsl/q32/fns/mix.rs` (NEW)

Create standalone mix functions:

- `mix_q32(a: Q32, b: Q32, t: Q32) -> Q32` - Scalar mix: `a + t * (b - a)`
- `mix_vec2(a: Vec2Q32, b: Vec2Q32, t: Vec2Q32) -> Vec2Q32` - Component-wise mix
- `mix_vec3(a: Vec3Q32, b: Vec3Q32, t: Vec3Q32) -> Vec3Q32` - Component-wise mix
- `mix_vec4(a: Vec4Q32, b: Vec4Q32, t: Vec4Q32) -> Vec4Q32` - Component-wise mix

### File: `glsl/q32/types/q32.rs` (UPDATE)

Add method:

- `mix(self, other: Q32, t: Q32) -> Q32` - Delegates to `mix_q32()`

### File: `glsl/q32/types/vec2_q32.rs` (UPDATE)

Add method:

- `mix(self, other: Vec2Q32, t: Vec2Q32) -> Vec2Q32` - Delegates to `mix_vec2()`

### File: `glsl/q32/types/vec3_q32.rs` (UPDATE)

Add method:

- `mix(self, other: Vec3Q32, t: Vec3Q32) -> Vec3Q32` - Delegates to `mix_vec3()`

### File: `glsl/q32/types/vec4_q32.rs` (UPDATE)

Add method:

- `mix(self, other: Vec4Q32, t: Vec4Q32) -> Vec4Q32` - Delegates to `mix_vec4()`

### File: `glsl/q32/fns/mod.rs` (UPDATE)

Export `mix` module.

## Success Criteria

- All mix functions implemented
- All mix methods added to types
- Functions use `#[inline(always)]` for performance
- Code compiles without errors
- Code formatted with `cargo +nightly fmt`

## Code Organization

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete solution"
- Avoid emoticons
- Use measured, factual descriptions of what was implemented
