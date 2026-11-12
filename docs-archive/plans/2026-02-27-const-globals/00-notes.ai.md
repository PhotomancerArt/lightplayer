# Plan: Const Spec Filetest Suite

## Scope of Work

Create a **spec-mirroring, feature-based filetest suite** for GLSL const and constant expressions. The suite will:

1. Mirror the GLSL spec (variables.adoc §§ Constant Qualifier, Constant Expressions)
2. Be organized by **feature** so each feature maps cleanly to a future implementation plan
3. Live in a dedicated `const/` category, **separate from globals**
4. Provide clean validation targets as const support is implemented

**This plan does NOT implement const support.** It establishes the test foundation. Implementation plans will reference these filetests as pass criteria.

## Spec Reference

- **variables.adoc** §4.3.3 "Constant Qualifier": const semantics (must init, read-only)
- **variables.adoc** §4.3.3.1 "Constant Expressions": what forms a constant expression

## Proposed Directory Structure

Feature-based layout. Each subdirectory = one feature/plan boundary.

```
filetests/const/
├── qualifier/              # Const qualifier semantics (spec §4.3.3)
│   ├── must-init.glsl      # Const must be initialized
│   ├── readonly.glsl      # Const is read-only; read allowed
│   └── write-error.glsl    # Write to const is compile error
│
├── expression/             # Constant expression forms (spec §4.3.3.1)
│   ├── literal.glsl       # Literal values (5, true, 3.14)
│   ├── operators.glsl     # Operators on const (+, -, *, /, %)
│   ├── constructors.glsl  # vec/mat constructors with const args
│   ├── reference.glsl     # Const references other const (ordering)
│   └── unary.glsl         # Unary minus, etc.
│
├── builtin/                # Built-in functions in const init (spec list)
│   ├── geometric.glsl     # length, dot, normalize
│   ├── trig.glsl          # radians, degrees, sin, cos, asin, acos
│   ├── exp.glsl           # pow, exp, log, exp2, log2, sqrt, inversesqrt
│   └── common.glsl        # abs, sign, floor, trunc, round, ceil, mod, min, max, clamp
│
├── array-size/             # Constant integral expression for array dimensions
│   ├── const-int.glsl     # const int N = 5; float arr[N];
│   ├── const-expr.glsl    # const int N = 2+3; float arr[N];
│   ├── local.glsl         # local const as array size
│   ├── multidim.glsl      # const in multidim array dims (from declare-multidim-const)
│   ├── struct-field.glsl  # const in struct array fields (from struct-array-const-size)
│   └── param.glsl         # const in param array size (from param-array-const-size)
│
├── scope/                  # Global vs local const
│   ├── global.glsl        # Global const declaration and use
│   └── local.glsl        # Local const (inside function)
│
└── errors/                 # Invalid const (compile errors) — uses test error + inline expected-error
    ├── user-func.glsl     # User-defined func in const init → error
    └── non-const-init.glsl # Non-constant expr in init → error
```

## Error Test Format (const/errors/)

Error tests use `// test error` and **inline** `// expected-error` directives (commit f66023c):

```
// test error
// target riscv32.q32

// Non-constant expression in const init — spec requires constant expression
float non_const = 1.0;
const float BAD = non_const;  // expected-error {{initializer must be constant expression}}
```

Syntax: `// expected-error [E0xxx:] {{message}}` or `// expected-error@+N [E0xxx:] {{message}}` for line offset. Message/code optional but at least one required.

## File Conventions

- **Header block**: First lines cite spec section and describe the file's scope
- **One concept per file** where practical; avoid mixing features
- **expect-fail** on all run directives until implementation exists
- **Minimal, focused tests** — enough to validate, not exhaust

Example header:
```
// test run
// target riscv32.q32

// Spec: variables.adoc §4.3.3 "Constant Qualifier"
// Const variables must be initialized at declaration.
```

## Migration from global/

Move and **split** existing const tests into the new structure:

| Current file | Target | Notes |
|--------------|--------|-------|
| global/const-expression.glsl | Split | → expression/literal, operators, constructors, reference, builtin/geometric |
| global/const-must-init.glsl | qualifier/must-init.glsl | Merge with init parts of others |
| global/const-readonly.glsl | qualifier/readonly.glsl | |
| global/declare-const.glsl | scope/global.glsl | Types + declaration |
| global/initialize-const.glsl | Merged into expression/* | Split by form |
| global/edge-const-write-error.glsl | qualifier/write-error.glsl | |
| global/access-read.glsl | Keep in global/ | Qualifier matrix; const is 1 of many |

## Array Tests

Array tests that use const for sizes move from `array/` to `const/array-size/`:

| Current file | Target |
|--------------|--------|
| array/declare-const-size.glsl | const/array-size/const-int.glsl (global + local) |
| array/declare-const-expression.glsl | const/array-size/const-expr.glsl |
| array/phase/8-constant-expressions.glsl | const/array-size/local.glsl |
| array/declare-multidim-const.glsl | const/array-size/multidim.glsl (or keep in array/ if focus is multidim) |
| array/struct-array-const-size.glsl | const/array-size/struct-field.glsl |
| array/param-array-const-size.glsl | const/array-size/param.glsl |

**Decision**: Array tests primarily about **const as integral expr** → `const/array-size/`. Tests primarily about array declaration syntax (multidim, struct) could stay in `array/` with a note that they require const; or move to `const/array-size/` for co-location. Recommend: move all const-in-array-size tests to `const/array-size/`.

## Function Const Parameters

**Leave in function/**: `param-const.glsl`, `edge-const-out-error.glsl`. Const *parameters* are a different spec section (function parameters), not constant expressions.

## Out of Scope

- Specialization constants (`layout(constant_id=...)`) — separate feature
- Built-in const variables (e.g. `gl_MaxVertexAttribs`) — implementation detail

## Error Test Infrastructure (resolved)

Error test support was added in f66023c: `TestType::Error`, inline `// expected-error`, and `test_error::run_error_test`. The `const/errors/` directory can be fully implemented—no need to defer or use placeholders.

## Summary

| Action | Count |
|--------|-------|
| New const/ category | 1 |
| Subdirectories | 6 (qualifier, expression, builtin, array-size, scope, errors) |
| Files to create | ~20 |
| Files to remove from global/ | 6 |
| Files to remove from array/ | 4–6 |

## Next Steps

After the filetest suite is in place, implementation plans can proceed feature-by-feature (e.g. qualifier + literals, then operators + constructors) with clear pass criteria: the corresponding const filetests must pass.
