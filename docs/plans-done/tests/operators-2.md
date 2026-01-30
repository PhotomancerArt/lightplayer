# Plan: Document Existing Operator Tests

## Overview

This plan documents the existing operator tests in `lightplayer/crates/lp-glsl-filetests/filetests/operators/`. These tests cover increment and decrement operators (pre/post) for scalars, vectors, matrices, and components.

## Existing Test Structure

The operator tests are organized in a flat structure:

```
operators/
├── for-loop-postinc.glsl
├── for-loop-preinc.glsl
├── incdec-component.glsl
├── incdec-edge-cases.glsl
├── incdec-matrix-column.glsl
├── incdec-matrix-element.glsl
├── incdec-matrix.glsl
├── incdec-scalar.glsl
├── incdec-vector.glsl
├── postdec-scalar-float.glsl
├── postdec-scalar-int.glsl
├── postinc-component.glsl
├── postinc-mat2.glsl
├── postinc-vec2.glsl
├── postinc-vec3.glsl
├── predec-component.glsl
├── predec-scalar-float.glsl
├── predec-scalar-int.glsl
├── predec-vec2.glsl
└── preinc-component.glsl
```

## Test Coverage

### Scalar Increment/Decrement

**incdec-scalar.glsl**: Scalar increment/decrement
- Pre-increment (`++x`)
- Post-increment (`x++`)
- Pre-decrement (`--x`)
- Post-decrement (`x--`)
- With float and int types

**preinc-scalar-float.glsl**: Pre-increment with float
- `++float` - increment and return new value
- Must be on lvalue

**postinc-scalar-float.glsl**: Post-increment with float (if exists)
- `float++` - return old value, then increment
- Must be on lvalue

**predec-scalar-float.glsl**: Pre-decrement with float
- `--float` - decrement and return new value
- Must be on lvalue

**postdec-scalar-float.glsl**: Post-decrement with float
- `float--` - return old value, then decrement
- Must be on lvalue

**predec-scalar-int.glsl**: Pre-decrement with int
- `--int` - decrement and return new value
- Must be on lvalue

**postdec-scalar-int.glsl**: Post-decrement with int
- `int--` - return old value, then decrement
- Must be on lvalue

### Vector Increment/Decrement

**incdec-vector.glsl**: Vector increment/decrement
- Pre/post increment/decrement on vectors
- Component-wise operations
- vec2, vec3, vec4

**postinc-vec2.glsl**: Post-increment with vec2
- `vec2++` - return old value, then increment all components
- Component-wise operation

**postinc-vec3.glsl**: Post-increment with vec3
- `vec3++` - return old value, then increment all components
- Component-wise operation

**predec-vec2.glsl**: Pre-decrement with vec2
- `--vec2` - decrement all components, return new value
- Component-wise operation

### Matrix Increment/Decrement

**incdec-matrix.glsl**: Matrix increment/decrement
- Pre/post increment/decrement on matrices
- Component-wise operations
- mat2, mat3, mat4

**postinc-mat2.glsl**: Post-increment with mat2
- `mat2++` - return old value, then increment all elements
- Component-wise operation

**incdec-matrix-column.glsl**: Matrix column increment/decrement
- Increment/decrement matrix columns
- Column-major order

**incdec-matrix-element.glsl**: Matrix element increment/decrement
- Increment/decrement individual matrix elements
- Element access patterns

### Component Increment/Decrement

**incdec-component.glsl**: Component increment/decrement
- Increment/decrement vector components
- `vec.x++`, `vec[0]++`, etc.
- Component access patterns

**preinc-component.glsl**: Pre-increment component
- `++vec.x` - increment component, return new value
- Component access

**postinc-component.glsl**: Post-increment component
- `vec.x++` - return old value, then increment component
- Component access

**predec-component.glsl**: Pre-decrement component
- `--vec.x` - decrement component, return new value
- Component access

### For Loop Integration

**for-loop-preinc.glsl**: Pre-increment in for loops
- `for (int i = 0; i < 10; ++i)`
- Pre-increment in loop expression
- Loop behavior

**for-loop-postinc.glsl**: Post-increment in for loops
- `for (int i = 0; i < 10; i++)`
- Post-increment in loop expression
- Loop behavior (same as pre-increment in this context)

### Edge Cases

**incdec-edge-cases.glsl**: Edge cases for increment/decrement
- Edge case behaviors
- Special values
- Boundary conditions

## Related Tests

**type_errors/incdec-bool.glsl**: Increment/decrement bool - compile error
- `++bool` - compile error
- `--bool` - compile error
- Bool cannot be incremented/decremented

**type_errors/incdec-non-lvalue.glsl**: Increment/decrement non-lvalue - compile error
- `++(x + y)` - compile error
- Must be on lvalue

**type_errors/incdec-nested.glsl**: Nested increment/decrement edge cases
- Complex expressions
- Evaluation order

## Missing Coverage

The following operator features may need additional tests:

1. **Compound assignment operators** - `+=`, `-=`, `*=`, `/=`, etc.
2. **Bitwise operators** - `&`, `|`, `^`, `<<`, `>>` (for integers)
3. **Modulo operator** - `%` (for integers)
4. **Logical operators** - `&&`, `||`, `^^` (for bool)
5. **Comparison operators** - `<`, `<=`, `>`, `>=` (covered in type tests)
6. **Equality operators** - `==`, `!=` (covered in type tests)
7. **Unary operators** - `+`, `-`, `!`, `~` (partially covered)

## GLSL Spec References

- **operators.adoc**: Increment/Decrement operators
- Key sections:
  - Pre-increment/decrement
  - Post-increment/decrement
  - Lvalue requirements
  - Component-wise operations
  - Type restrictions






