# Phase 3: Implement Mat4Q32

## Description

Implement the `Mat4Q32` type with all operations, methods, and operator overloads.

## Implementation

Create `lp-glsl/lp-glsl-builtins/src/util/mat4_q32.rs` with:

- `Mat4Q32` struct with `m: [Q32; 16]` field (column-major storage)
- Construction methods: `new()`, `from_f32()`, `from_vec4()`, `identity()`, `zero()`
- Element access: `get()`, `set()`, `col0()`, `col1()`, `col2()`, `col3()`
- Operations: `mul()` (matrix-matrix), `mul_vec4()` (matrix-vector), `transpose()`,
  `determinant()` (Laplace expansion), `inverse()`
- Operator overloads: `Add`, `Sub`, `Mul<Mat4Q32>`, `Mul<Vec4Q32>`, `Mul<Q32>`, `Div<Q32>`, `Neg`

All operations use Q32's fast operators directly. Determinant uses Laplace expansion (cofactor
expansion).

Update `util/mod.rs` to export `Mat4Q32`.

## Success Criteria

- `Mat4Q32` type is defined and exported
- All construction methods work correctly
- All math operations work correctly (including determinant using Laplace expansion)
- Element access methods work correctly
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
