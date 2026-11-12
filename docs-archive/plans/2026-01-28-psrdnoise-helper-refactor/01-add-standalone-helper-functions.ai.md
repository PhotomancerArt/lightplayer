# Phase 1: Add standalone helper functions

## Description

Implement standalone helper functions in `glsl/q32/fns/` for component-wise operations on vectors. These functions will be used by both the wrapper type methods and directly in psrdnoise implementations.

## Implementation

Create files in `glsl/q32/fns/`:

- `floor.rs` - floor_vec2, floor_vec3, floor_vec4
- `fract.rs` - fract_vec2, fract_vec3, fract_vec4
- `step.rs` - step_vec2, step_vec3, step_vec4
- `min.rs` - min_vec2, min_vec3, min_vec4
- `max.rs` - max_vec2, max_vec3, max_vec4
- `mod.rs` - mod_vec2, mod_vec3, mod_vec4, mod_vec3_scalar, mod_vec4_scalar
- `sin.rs` - sin_vec2, sin_vec3, sin_vec4
- `cos.rs` - cos_vec2, cos_vec3, cos_vec4
- `sqrt.rs` - sqrt_vec2, sqrt_vec3, sqrt_vec4

Update `glsl/q32/fns/mod.rs` to export all functions.

## Success Criteria

- All standalone functions implemented
- Functions use `#[inline(always)]` for performance
- Functions match GLSL semantics exactly
- Basic tests added for each function
- Code compiles without errors

## Code Organization

- Place helper utility functions at the bottom of files
- Place more abstract things, entry points, and tests first
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing
- Ensure consistent formatting across modified files

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete solution"
- Avoid emoticons
- Code is never done, never perfect, never fully ready, never fully complete
- Use measured, factual descriptions of what was implemented
