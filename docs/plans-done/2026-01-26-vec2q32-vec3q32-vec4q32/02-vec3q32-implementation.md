# Phase 2: Implement Vec3Q32

## Description

Implement the `Vec3Q32` type with all operations, methods, and GLSL-style swizzle methods.

## Implementation

Create `lp-glsl/lp-glsl-builtins/src/util/vec3_q32.rs` with:

- `Vec3Q32` struct with `x: Q32`, `y: Q32`, and `z: Q32` fields
- Construction methods: `new()`, `from_f32()`, `from_i32()`, `zero()`, `one()`
- Math operations: `dot()`, `cross()` (returns `Vec3Q32`), `length_squared()`, `length()`,
  `distance()`, `normalize()`, `reflect()`
- Component operations: `mul_comp()`, `div_comp()`, `clamp()`
- GLSL-style swizzle methods:
    - Component accessors: `.x()`, `.y()`, `.z()`, `.r()`, `.g()`, `.b()`
    - 2-component swizzles returning `Vec2Q32`: `.xy()`, `.xz()`, `.yz()`, `.yx()`, `.zx()`,
      `.zy()`, and RGBA variants
    - 3-component swizzles: `.xyz()`, `.xzy()`, `.yxz()`, `.yzx()`, `.zxy()`, `.zyx()`, and RGBA
      variants
- Operator overloads: `Add`, `Sub`, `Mul<Q32>`, `Div<Q32>`, `Neg`

All operations use Q32's fast operators directly. `length()` uses `__lp_q32_sqrt` builtin.

Update `util/mod.rs` to export `Vec3Q32`.

## Success Criteria

- `Vec3Q32` type is defined and exported
- All construction methods work correctly
- All math operations work correctly (including `cross()` returning `Vec3Q32`)
- `reflect()` method works correctly
- All swizzle methods work correctly (including 2-component swizzles returning `Vec2Q32`)
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
- Avoid overly optimistic language like "comprehensive", "fully production ready", "complete
  solution"
- Avoid emoticons
- Use measured, factual descriptions of what was implemented
