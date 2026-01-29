# Phase 1: Implement Vec2Q32

## Description

Implement the `Vec2Q32` type with all operations, methods, and GLSL-style swizzle methods.

## Implementation

Create `lp-glsl/crates/lp-builtins/src/util/vec2_q32.rs` with:

- `Vec2Q32` struct with `x: Q32` and `y: Q32` fields
- Construction methods: `new()`, `from_f32()`, `from_i32()`, `zero()`, `one()`
- Math operations: `dot()`, `cross()`, `length_squared()`, `length()`, `distance()`, `normalize()`
- Component operations: `mul_comp()`, `div_comp()`
- GLSL-style swizzle methods:
  - Component accessors: `.x()`, `.y()`, `.r()`, `.g()`, `.s()`, `.t()`
  - 2-component swizzles: `.xx()`, `.xy()`, `.yx()`, `.yy()`, and RGBA/STPQ variants
- Operator overloads: `Add`, `Sub`, `Mul<Q32>`, `Div<Q32>`, `Neg`

All operations use Q32's fast operators directly. `length()` uses `__lp_q32_sqrt` builtin.

Update `util/mod.rs` to export `Vec2Q32`.

## Success Criteria

- `Vec2Q32` type is defined and exported
- All construction methods work correctly
- All math operations work correctly
- All swizzle methods work correctly
- Operator overloads work correctly
- Code compiles without errors
- Code is formatted with `cargo +nightly fmt`
- Code is `no_std` compatible

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
