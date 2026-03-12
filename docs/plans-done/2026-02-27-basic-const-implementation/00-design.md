# Design: Basic Const Support in GLSL Compiler

## Scope

Implement basic `const` support so that:
- Global const declarations (e.g. `const float PI = 3.14159;`) compile and can be used in functions
- Local const is properly validated (read-only, must-init)
- Simple constant expressions (literals, +, -, *, /, constructors) are supported from Phase 1
- Array size constant expressions (Phase 2), error diagnostics (must-init, non-const init)

## File Structure

```
lp-glsl/lp-glsl-compiler/src/
├── frontend/
│   ├── semantic/
│   │   ├── mod.rs
│   │   ├── scope.rs                    # UPDATE: add StorageClass::Const
│   │   ├── const_eval.rs               # NEW: ConstValue enum, eval_constant_expr
│   │   ├── passes/
│   │   │   ├── function_extraction.rs  # UPDATE: (or new pass) process globals
│   │   │   └── global_const_pass.rs    # NEW: collect global consts, populate TypedShader
│   │   └── ...
│   └── codegen/
│       ├── context.rs                  # UPDATE: accept global_constants, lookup const by value
│       ├── expr/
│       │   └── variable.rs             # UPDATE: resolve const ref → emit value directly
│       └── stmt/
│           └── declaration.rs          # UPDATE: handle StorageClass::Const, must-init
└── error.rs                            # UPDATE: add ErrorCode for const init
```

## Conceptual Architecture

```
                    TranslationUnit (glsl AST)
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  GlobalConstPass (new)                                            │
│  - Walk ExternalDeclaration::Declaration                          │
│  - Filter by const qualifier (FullySpecifiedType)                  │
│  - Eval initializer via const_eval::eval_constant_expr           │
│  - Store in TypedShader.global_constants: HashMap<Name, ConstValue│
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  TypedShader                                                      │
│  + global_constants: HashMap<String, ConstValue>                  │
│  + main_function, user_functions, function_registry               │
└─────────────────────────────────────────────────────────────────┘
                              │
         ┌────────────────────┼────────────────────┐
         ▼                    ▼                    ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────────────┐
│  Validation     │  │  Codegen        │  │  Array size parsing      │
│  - Pre-populate │  │  - Pre-populate │  │  (Phase 2)              │
│    symbol table │  │    ctx with     │  │  - parse_array_dims     │
│    from         │  │    global_consts│  │    accepts const ref    │
│    global_consts│  │  - Variable ref │  │  - Resolve via const_env│
│  - StorageClass │  │    → if const   │  └─────────────────────────┘
│    ::Const      │  │    → emit value │
│  - Reject write │  │    directly     │
└─────────────────┘  └─────────────────┘

const_eval module:
  ConstValue = Int | UInt | Float | Bool | Vec2 | Vec3 | Vec4 | Mat2 | ...
  eval_constant_expr(expr, const_env) -> Result<ConstValue>
    - Literals → ConstValue
    - Var ref in env → lookup
    - Binary op (+, -, *, /, %) on const operands → fold
    - Unary - on const → fold
    - Constructor (vec2, vec3, ...) with const args → fold
```

## Main Components

- **const_eval**: Evaluates constant expressions given a const environment. Supports literals, binary ops, unary minus, constructors, and const variable references.
- **GlobalConstPass**: New semantic pass that runs before or alongside function extraction. Collects global const declarations, evaluates initializers, populates `TypedShader.global_constants`.
- **TypedShader.global_constants**: Canonical map of global const name → evaluated value. Injected into per-function scope for validation and codegen.
- **StorageClass::Const**: Tags variables as const. Enables must-init check, write rejection in LValue resolution.
- **Codegen variable resolution**: When resolving a variable reference, if it's a const (from global_constants or local with Const storage), emit the value directly instead of loading from a slot.
