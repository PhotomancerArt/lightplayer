# Phase 3: Add methods to Vec3Q32

## Description

Add GLSL-style methods to Vec3Q32 that delegate to the standalone helper functions. Also add extended swizzle operations needed for psrdnoise (like `.xyx`, `.yzz`).

## Implementation

Update `glsl/q32/types/vec3_q32.rs` to add:

**Methods:**

- `floor(self) -> Vec3Q32` - Component-wise floor
- `fract(self) -> Vec3Q32` - Component-wise fractional part
- `step(self, edge: Vec3Q32) -> Vec3Q32` - Component-wise step function
- `min(self, other: Vec3Q32) -> Vec3Q32` - Component-wise minimum
- `max(self, other: Vec3Q32) -> Vec3Q32` - Component-wise maximum
- `mod(self, other: Vec3Q32) -> Vec3Q32` - Component-wise modulo
- `mod_scalar(self, y: Q32) -> Vec3Q32` - Modulo with scalar

**Extended swizzles:**

- `xyx(self) -> Vec3Q32` - Swizzle (x, y, x) - needed for `step(f0.xyx, f0.yzz)`
- `yzz(self) -> Vec3Q32` - Swizzle (y, z, z) - needed for `step(f0.xyx, f0.yzz)`
- `yzx(self) -> Vec3Q32` - Swizzle (y, z, x) - for component access patterns

Each method should delegate to the corresponding standalone function in `glsl/q32/fns/`.

## Success Criteria

- All methods implemented and delegate to standalone functions
- Extended swizzles implemented
- Methods use `#[inline(always)]` for performance
- Methods match GLSL semantics exactly
- Tests added for each method and swizzle
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
