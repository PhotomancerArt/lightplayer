# Phase II: Naga WASM Feature Complete — Design

## Scope of work

Expand `lp-glsl-wasm` and `lp-glsl-naga` from scalar-only emission to full
feature set. Definition of done: `rainbow.glsl` renders in the web demo and
all existing wasm.q32 filetests pass (or are annotated as out-of-scope).

## File structure

```
lp-glsl/
├── lp-glsl-naga/
│   └── src/
│       ├── lib.rs               # UPDATE: LPFX prototype injection in compile()
│       └── builtins.rs          # NEW: LPFX prototype definitions, #line reset
├── lp-glsl-wasm/
│   └── src/
│       ├── lib.rs               # UPDATE: import section in emit pipeline
│       ├── emit.rs              # UPDATE: vectors, builtins, calls, control flow
│       ├── emit_vec.rs          # NEW: scalarized vector emission helpers
│       ├── emit_math.rs         # NEW: MathFunction → WASM (inline or import)
│       ├── emit_call.rs         # NEW: user function calls + LPFX import dispatch
│       ├── imports.rs           # NEW: WASM import section builder (builtins module)
│       ├── locals.rs            # UPDATE: vector locals (N consecutive), call result temps
│       ├── module.rs            # unchanged
│       ├── options.rs           # unchanged
│       └── types.rs             # UPDATE: vector type helpers
└── lp-glsl-filetests/
    └── src/test_run/
        └── wasm_runner.rs       # MINOR: fix any new type dispatch issues
```

## Conceptual architecture

```
GLSL source
    │
    ▼
lp-glsl-naga::compile()
    ├── prepend LPFX prototypes
    ├── append #line 1
    ├── append dummy main() if needed
    ├── naga::front::glsl::Frontend::parse()
    └── extract FunctionInfo (now including vec types)
    │
    ▼
naga::Module
    │
    ▼
lp-glsl-wasm::emit_module()
    │
    ├── Build import section:
    │   ├── Scan all functions for MathFunction refs → builtin imports
    │   ├── Scan for Call statements to LPFX functions → builtin imports
    │   └── If any imports: add env.memory import
    │
    ├── Build function section + type section:
    │   ├── Import functions get type indices 0..N-1
    │   └── User functions get type indices N..
    │
    ├── For each user function:
    │   ├── LocalAlloc: param locals, user locals (vec = N slots), scratch, call-result temps
    │   ├── emit_block(body):
    │   │   ├── Statement::Emit → no-op (expressions emitted on demand)
    │   │   ├── Statement::Store → emit value, local.set (vec: N stores)
    │   │   ├── Statement::Return → emit value (vec: N values), return
    │   │   ├── Statement::Call → emit args, call $idx, store result temps
    │   │   ├── Statement::If → emit cond, if/else/end
    │   │   ├── Statement::Loop → block/loop/block, body, continuing, break_if
    │   │   ├── Statement::Break → br 2
    │   │   └── Statement::Continue → br 0
    │   └── emit_expr(h):
    │       ├── Literal → const (vec: N consts)
    │       ├── FunctionArgument → local.get (vec: N gets)
    │       ├── Load { LocalVariable } → local.get (vec: N gets)
    │       ├── Binary → emit left, emit right, op (vec: per-component)
    │       ├── Math → inline or call import (vec: per-component or pass N args)
    │       ├── Compose → emit each component
    │       ├── Splat → emit scalar, then duplicate N times via local.tee
    │       ├── Swizzle → emit vec, store to temps, reorder via local.get
    │       ├── AccessIndex → emit vec, extract component k
    │       ├── Access → emit vec, emit index, dynamic extraction
    │       ├── Select → emit accept, reject, cond, select (vec: per-component)
    │       ├── CallResult → local.get from call-result temps
    │       └── Relational → all/any for vectors
    │
    └── Assemble: types + imports + functions + exports + code → WASM bytes
```

## Main components

### lp-glsl-naga builtins.rs

- `fn lpfx_prototypes() -> &'static str`: returns GLSL forward declarations
  for all known LPFX functions
- `fn prepend_builtins(source: &str) -> String`: prepends prototypes +
  `#line 1` before user source
- Called from `compile()` before `parse_glsl()`

### emit_vec.rs — vector scalarization

- `fn vector_dim(module, func, expr) -> Option<u32>`: returns None for
  scalars, Some(2..4) for vectors
- `fn emit_vec_expr(module, func, expr, wasm_fn, mode, alloc) -> Result<(), String>`:
  dispatches to per-component emission for vector expressions
- `fn emit_vec_binary(op, kind, mode, dim, wasm_fn, alloc)`: emits N
  independent binary ops using scratch locals for operand shuffling
- `fn emit_compose(module, func, ty, components, wasm_fn, mode, alloc)`:
  emits each component expression in order
- `fn emit_splat(module, func, value, size, wasm_fn, mode, alloc)`:
  emits scalar, stores to temp, then N `local.get`s
- `fn emit_swizzle(module, func, vector, pattern, size, wasm_fn, mode, alloc)`:
  emits source vector to temps, then reorders

### emit_math.rs — MathFunction dispatch

- `fn emit_math(fun, args, mode, wasm_fn, module, func, alloc, imports)`:
  dispatches `MathFunction` variants
- Float mode inline: `Floor` → `f32.floor`, `Sqrt` → `f32.sqrt`, etc.
- Q32 mode: everything → import call
- Mixed: `Abs` inline for both, `Min`/`Max` → `f32.min`/`f32.max` or
  import for Q32
- Returns the import index if an import was used

### emit_call.rs — function calls

- `fn emit_user_call(func_handle, arguments, result, ...)`: emits args,
  `call $idx`, stores result to temps if vector
- `fn emit_lpfx_call(func_name, arguments, result, ...)`: same but
  resolves to import index
- `fn is_lpfx_function(name) -> bool`: checks `lpfx_` prefix

### imports.rs — import section

- Pre-scan all functions to collect required imports
- Build `ImportSection` with `(module: "builtins", name: "__lp_*", type_idx)`
- Track `HashMap<String, u32>` mapping import name → WASM func index
- Add `env.memory` import if any builtins are used

### locals.rs updates

- Vector locals: `LocalVariable` with `TypeInner::Vector { size, .. }` →
  allocate `size` consecutive WASM locals
- Call result temps: for each `Statement::Call { result: Some(h) }` where
  the result type is vector, allocate N extra temps
- `resolve_local_variable()` returns base index; callers add component offset
