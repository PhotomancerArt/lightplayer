# Stage I: Builtin Naming Convention — Design

## Scope of work

Rename all builtin symbols to `__lp_<module>_<fn>_<mode>`, split the single
`std.math` LPIR import module into `glsl` and `lpir`, make `BuiltinId`
self-describing with `module()`, `fn_name()`, `mode()` methods. Update all
consumers. All WASM-path and LPIR-path tests pass after rename.

## File structure

```
lp-glsl/
├── lp-glsl-builtin-ids/src/
│   ├── lib.rs                          # UPDATE: regenerated BuiltinId enum
│   │                                   #   new variant names (LpGlslSinQ32, etc.)
│   │                                   #   new methods: module(), fn_name(), mode()
│   └── glsl_builtin_mapping.rs         # UPDATE: regenerated, new BuiltinId names
│                                       #   resolve_builtin_id matches "glsl"+"lpir"+"lpfx"
│
├── lp-glsl-builtins/src/builtins/
│   ├── q32/
│   │   ├── sin.rs                      # UPDATE: fn __lp_q32_sin → fn __lp_glsl_sin_q32
│   │   ├── add.rs                      # UPDATE: fn __lp_q32_add → fn __lp_lpir_fadd_q32
│   │   ├── sqrt.rs                     # UPDATE: fn __lp_q32_sqrt → fn __lp_lpir_fsqrt_q32
│   │   ├── ... (all 29 q32 files)      # UPDATE: renamed per Q2 classification
│   │   └── mod.rs                      # UPDATE: regenerated
│   └── lpfx/
│       ├── hash.rs                     # UPDATE: fn __lpfx_hash_1 → fn __lp_lpfx_hash_1
│       ├── generative/fbm/...          # UPDATE: __lpfx_ → __lp_lpfx_ prefix
│       ├── ... (all lpfx files)        # UPDATE: prefix rename
│       └── ...
│
├── lp-glsl-builtins-gen-app/src/
│   └── main.rs                         # UPDATE: generator logic
│                                       #   - remove old cranelift registry/mapping output
│                                       #   - add Module enum, mode detection
│                                       #   - generate module()/fn_name()/mode() methods
│                                       #   - update glsl_builtin_mapping generation
│
├── lp-glsl-builtins-emu-app/src/
│   └── builtin_refs.rs                 # UPDATE: regenerated (new names)
│
├── lp-glsl-builtins-wasm/src/
│   └── builtin_refs.rs                 # UPDATE: regenerated (new names)
│
├── lp-glsl-naga/src/
│   ├── lower.rs                        # UPDATE: split register_std_math_imports
│   │                                   #   "glsl" for trig/exp/etc, "lpir" for sqrt
│   ├── lower_ctx.rs                    # UPDATE: import_map keys "glsl::" / "lpir::"
│   ├── lower_math.rs                   # UPDATE: push_std_math key format
│   └── std_math_handler.rs             # UPDATE: rename, dispatch "glsl"+"lpir"
│
├── lp-glsl-wasm/src/
│   ├── emit/imports.rs                 # UPDATE: resolve_builtin_id matches
│   │                                   #   "glsl", "lpir", "lpfx"
│   └── codegen/
│       └── builtin_wasm_import_types.rs # UPDATE: regenerated
│
├── lpir/src/
│   └── tests/                          # UPDATE: @std.math:: → @glsl:: / @lpir::
│       ├── interp.rs
│       └── ...
│
├── lp-glsl-naga/tests/
│   ├── lower_print.rs                  # UPDATE: assertion strings
│   └── lower_interp.rs                 # UPDATE: CombinedImports dispatch
│
└── lp-glsl-cranelift/                  # NOT UPDATED — accept breakage
    └── src/backend/builtins/
        ├── registry.rs                 # STALE: generator no longer emits here
        └── mapping.rs                  # STALE: generator no longer emits here
```

## Conceptual architecture

```
               Naming Convention
               ═════════════════

    __lp_<module>_<fn>_<mode>
         │         │      │
         │         │      └── _q32 / _f32 / (none for mode-independent)
         │         └── function name (sin, fadd, fbm2, hash_1)
         └── module: lpir / glsl / lpfx


               Module Classification
               ═════════════════════

    ┌─────────────────────────────────────────────────────┐
    │ lpir  — has matching LPIR opcode                    │
    │   fadd, fsub, fmul, fdiv, fsqrt, fnearest          │
    │   Naga imports as @lpir::{name}                     │
    ├─────────────────────────────────────────────────────┤
    │ glsl  — GLSL std.450, no matching opcode            │
    │   sin, cos, pow, exp, round, fma, mod, ...          │
    │   Naga imports as @glsl::{name}                     │
    ├─────────────────────────────────────────────────────┤
    │ lpfx  — LightPlayer effects                         │
    │   fbm2, snoise3, hash_1, saturate, ...              │
    │   Naga imports as @lpfx::{name}                     │
    └─────────────────────────────────────────────────────┘


               BuiltinId (enum, self-describing)
               ═════════════════════════════════

    BuiltinId::LpGlslSinQ32
        .name()    → "__lp_glsl_sin_q32"
        .module()  → Module::Glsl
        .fn_name() → "sin"
        .mode()    → Some(Mode::Q32)

    BuiltinId::LpLpirFaddQ32
        .name()    → "__lp_lpir_fadd_q32"
        .module()  → Module::Lpir
        .fn_name() → "fadd"
        .mode()    → Some(Mode::Q32)

    BuiltinId::LpLpfxHash1
        .name()    → "__lp_lpfx_hash_1"
        .module()  → Module::Lpfx
        .fn_name() → "hash_1"
        .mode()    → None


               Import Resolution Flow
               ══════════════════════

    GLSL source: sin(x)
         │
         ▼  Naga → LPIR lowering
    ImportDecl { module: "glsl", func: "sin", ... }
         │
         ▼  WASM emitter (imports.rs)
    resolve_builtin_id("glsl", "sin") → BuiltinId::LpGlslSinQ32
         │
         ▼
    WASM import: ("builtins", "__lp_glsl_sin_q32")


    GLSL source: sqrt(x)
         │
         ▼  Naga → LPIR lowering
    ImportDecl { module: "lpir", func: "sqrt", ... }
         │
         ▼  WASM emitter (imports.rs)
    resolve_builtin_id("lpir", "sqrt") → BuiltinId::LpLpirFsqrtQ32
         │
         ▼
    WASM import: ("builtins", "__lp_lpir_fsqrt_q32")
```

## Main components and interactions

1. **`lp-glsl-builtins`** — source of truth for builtin implementations.
   Function identifiers = ELF symbol names. Renamed in place.

2. **`lp-glsl-builtins-gen-app`** — walks builtins source, generates
   `BuiltinId` enum and consumer files. Updated to:
   - Derive module/fn_name/mode from new naming convention
   - Generate `module()`, `fn_name()`, `mode()` methods
   - Stop emitting into `lp-glsl-cranelift`
   - Update `glsl_builtin_mapping` generation for new variant names

3. **`lp-glsl-builtin-ids`** — regenerated. New variant names, new methods,
   new `Module` and `Mode` enums.

4. **`lp-glsl-naga` lowering** — `register_std_math_imports` splits into
   `"glsl"` and `"lpir"` module names. Import map keys change accordingly.

5. **`lp-glsl-wasm` import resolution** — `resolve_builtin_id` matches on
   `"glsl"`, `"lpir"`, `"lpfx"` module names.

6. **Interpreter handler** — renamed, dispatches on `"glsl"` and `"lpir"`.
