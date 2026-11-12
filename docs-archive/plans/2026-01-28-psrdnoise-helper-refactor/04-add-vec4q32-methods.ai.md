# Phase 4: Add methods to Vec4Q32

## Description

Add GLSL-style methods to Vec4Q32 that delegate to the standalone helper functions. Also add constructor helpers for creating Vec4Q32 from Vec3Q32 + scalar, which is needed for psrdnoise hash computations.

## Implementation

Update `glsl/q32/types/vec4_q32.rs` to add:

**Methods:**

- `floor(self) -> Vec4Q32` - Component-wise floor
- `fract(self) -> Vec4Q32` - Component-wise fractional part
- `step(self, edge: Vec4Q32) -> Vec4Q32` - Component-wise step function
- `min(self, other: Vec4Q32) -> Vec4Q32` - Component-wise minimum
- `max(self, other: Vec4Q32) -> Vec4Q32` - Component-wise maximum
- `mod(self, other: Vec4Q32) -> Vec4Q32` - Component-wise modulo
- `mod_scalar(self, y: Q32) -> Vec4Q32` - Modulo with scalar

**Constructors:**

- `from_vec3_scalar(v: Vec3Q32, w: Q32) -> Vec4Q32` - Create Vec4 from Vec3 + scalar (needed for `vec4(v0.x, v1.x, v2.x, v3.x)` patterns)

**Swizzles:**

- Verify `xyz(self) -> Vec3Q32` exists (extract xyz as Vec3Q32)

Each method should delegate to the corresponding standalone function in `glsl/q32/fns/`.

## Success Criteria

- All methods implemented and delegate to standalone functions
- Constructor helper implemented
- Methods use `#[inline(always)]` for performance
- Methods match GLSL semantics exactly
- Tests added for each method and constructor
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
