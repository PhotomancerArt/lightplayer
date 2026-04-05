# Naga WASM Scaffold (Roadmap Phase I) — Design

## Scope of work

Create two crates and update one:

1. **NEW** `lp-shader/lp-glsl-naga/` — Naga-based GLSL frontend
2. **REWRITE** `lp-shader/lp-glsl-wasm/` — WASM backend consuming naga::Module
3. **UPDATE** `lp-shader/lp-glsl-filetests/` — wasm_runner uses new types

Scalar filetests (`scalar/float/op-add.glsl` etc.) pass on `wasm.q32` target.

## File structure

```
lp-shader/
├── lp-glsl-naga/                    # NEW
│   ├── Cargo.toml                   # naga (glsl-in), no_std
│   └── src/
│       └── lib.rs                   # compile(), NagaModule, GlslType, FloatMode, FunctionInfo
├── lp-glsl-wasm/                    # REWRITE
│   ├── Cargo.toml                   # dep: lp-glsl-naga (replaces lp-glsl-frontend)
│   └── src/
│       ├── lib.rs                   # glsl_wasm() entry point
│       ├── emit.rs                  # emit_module(), emit_function(), emit_expr(), emit_stmt()
│       ├── locals.rs                # Local allocation from naga expression arena
│       ├── module.rs                # WasmModule, WasmExport (updated, no FunctionSignature)
│       ├── options.rs               # WasmOptions (uses lp_glsl_naga::FloatMode)
│       └── types.rs                 # Naga type → WasmValType mapping
└── lp-glsl-filetests/               # UPDATE
    └── src/test_run/
        ├── compile.rs               # wasm path uses new lp-glsl-wasm API
        └── wasm_runner.rs           # Uses new WasmExport/GlslType (no FunctionSignature)
```

## Conceptual architecture

```
                    lp-glsl-naga
                    ┌────────────────────────┐
  GLSL &str ──────▶│ naga::front::glsl      │
                    │                        │
                    │ Returns NagaModule:    │
                    │  - naga::Module        │
                    │  - Vec<FunctionInfo>   │
                    │  - FloatMode           │
                    └───────────┬────────────┘
                                │
                    lp-glsl-wasm│
                    ┌───────────▼────────────┐
                    │ emit_module()          │
                    │  for each function:    │
                    │    allocate locals     │
                    │    walk body stmts     │
                    │    emit_expr() recurse │
                    │                        │
                    │ Returns WasmModule:    │
                    │  - bytes: Vec<u8>      │
                    │  - exports: Vec<...>   │
                    └───────────┬────────────┘
                                │
                    filetests   │
                    ┌───────────▼────────────┐
                    │ wasm_runner.rs         │
                    │  wasmtime instantiate  │
                    │  call exported funcs   │
                    │  convert results       │
                    └────────────────────────┘
```

## Main components

### lp-glsl-naga

- `FloatMode` enum: `Q32` / `Float` (owned by this crate, not re-exported)
- `GlslType` enum: `Float`, `Int`, `UInt`, `Bool`, `Vec2`, `Vec3`, `Vec4`,
  `IVec2`..`IVec4`, `UVec2`..`UVec4`, `BVec2`..`BVec4`, `Void`
  (derived from naga `TypeInner`; used by wasm_runner for call dispatch)
- `FunctionInfo`: `name: String`, `params: Vec<(String, GlslType)>`,
  `return_type: GlslType`
- `NagaModule`: `module: naga::Module`, `functions: Vec<FunctionInfo>`
- `compile(source: &str, float_mode: FloatMode) → Result<NagaModule, ...>`

### lp-glsl-wasm (rewritten)

- `glsl_wasm(source, options) → Result<WasmModule, ...>`
- `emit_module(naga_module, options) → Vec<u8>`: walks each function,
  allocates WASM locals, emits instructions
- `emit_expr(expr_handle) → ()`: recursive, pushes one value onto WASM stack
- `emit_stmt(stmt)`: handles `Store`, `Emit`, `Return`, `Block`
- Local allocation: param-locals (mapped from naga FunctionArgument),
  expression-locals (one per emitted expression that needs storage)

### lp-glsl-filetests (updated)

- `wasm_runner.rs`: uses `lp_glsl_wasm::GlslType` (from lp-glsl-naga,
  re-exported) instead of `lp_glsl_frontend::semantic::types::Type`
- `compile.rs`: same `glsl_wasm()` call signature, different types
