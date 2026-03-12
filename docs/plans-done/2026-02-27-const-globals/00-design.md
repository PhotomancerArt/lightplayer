# Design: Const Spec Filetest Suite

## Scope

Create a spec-mirroring, feature-based filetest suite for GLSL const and constant expressions. No implementation of const support in this plan.

## File Structure

```
lp-glsl/lp-glsl-filetests/filetests/
├── const/                         # NEW: const category
│   ├── qualifier/
│   │   ├── must-init.glsl
│   │   ├── readonly.glsl
│   │   └── write-error.glsl
│   ├── expression/
│   │   ├── literal.glsl
│   │   ├── operators.glsl
│   │   ├── constructors.glsl
│   │   ├── reference.glsl
│   │   └── unary.glsl
│   ├── builtin/
│   │   ├── geometric.glsl
│   │   ├── trig.glsl
│   │   ├── exp.glsl
│   │   └── common.glsl
│   ├── array-size/
│   │   ├── const-int.glsl
│   │   ├── const-expr.glsl
│   │   ├── local.glsl
│   │   ├── multidim.glsl
│   │   ├── struct-field.glsl
│   │   └── param.glsl
│   ├── scope/
│   │   ├── global.glsl
│   │   └── local.glsl
│   └── errors/
│       ├── user-func.glsl
│       └── non-const-init.glsl
├── global/                        # UPDATE: remove 6 const files
└── array/                         # UPDATE: remove 4–6 const-size files
```

## Conceptual Flow

```
Spec (variables.adoc)
    └── Constant Qualifier (§4.3.3)     → const/qualifier/
    └── Constant Expressions (§4.3.3.1) → const/expression/, const/builtin/
    └── Array size (integral expr)       → const/array-size/
    └── Scope (global vs local)         → const/scope/
    └── Invalid init (errors)           → const/errors/
```

Each subdirectory = one feature = one future implementation plan boundary.

## Main Components

- **qualifier/**: Must-init, readonly, write-error (spec §4.3.3)
- **expression/**: Literals, operators, constructors, reference, unary (spec §4.3.3.1)
- **builtin/**: Spec-listed builtins in const init (geometric, trig, exp, common)
- **array-size/**: Const integral expr as array dimensions
- **scope/**: Global vs local const
- **errors/**: `// test error` + inline `// expected-error` (f66023c)

## Validation

`just filetest const/` or equivalent — all tests run; run tests use `[expect-fail]` until const support is implemented; error tests pass when the compiler rejects invalid const.
