# Phase 2: Add cubic and quintic interpolation functions

## Description

Add `cubic()` and `quintic()` interpolation functions. These are used by gnoise for smooth interpolation between grid cell corners. Cubic is used for 2D gnoise, quintic for 3D gnoise.

## Implementation

### File: `glsl/q32/fns/cubic.rs` (NEW)

Create cubic interpolation functions:

- `cubic_q32(v: Q32) -> Q32` - Scalar cubic: `v * v * (3.0 - 2.0 * v)`
- `cubic_vec2(v: Vec2Q32) -> Vec2Q32` - Component-wise cubic
- `cubic_vec3(v: Vec3Q32) -> Vec3Q32` - Component-wise cubic
- `cubic_vec4(v: Vec4Q32) -> Vec4Q32` - Component-wise cubic

### File: `glsl/q32/fns/quintic.rs` (NEW)

Create quintic interpolation functions:

- `quintic_q32(v: Q32) -> Q32` - Scalar quintic: `v * v * v * (v * (v * 6.0 - 15.0) + 10.0)`
- `quintic_vec2(v: Vec2Q32) -> Vec2Q32` - Component-wise quintic
- `quintic_vec3(v: Vec3Q32) -> Vec3Q32` - Component-wise quintic
- `quintic_vec4(v: Vec4Q32) -> Vec4Q32` - Component-wise quintic

### File: `glsl/q32/fns/mod.rs` (UPDATE)

Export `cubic` and `quintic` modules.

## Success Criteria

- All cubic functions implemented
- All quintic functions implemented
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
