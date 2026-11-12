# Phase 2: Add methods to Vec2Q32

## Description

Add GLSL-style methods to Vec2Q32 that delegate to the standalone helper functions. This provides a clean API that matches GLSL usage patterns.

## Implementation

Update `glsl/q32/types/vec2_q32.rs` to add:

- `floor(self) -> Vec2Q32` - Component-wise floor
- `fract(self) -> Vec2Q32` - Component-wise fractional part
- `step(self, edge: Vec2Q32) -> Vec2Q32` - Component-wise step function
- `min(self, other: Vec2Q32) -> Vec2Q32` - Component-wise minimum
- `max(self, other: Vec2Q32) -> Vec2Q32` - Component-wise maximum
- `mod(self, other: Vec2Q32) -> Vec2Q32` - Component-wise modulo

Each method should delegate to the corresponding standalone function in `glsl/q32/fns/`.

## Success Criteria

- All methods implemented and delegate to standalone functions
- Methods use `#[inline(always)]` for performance
- Methods match GLSL semantics exactly
- Tests added for each method
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
