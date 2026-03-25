# Stage IV: JitModule API, GlslMetadata, and Compiler Orchestration — Design

## Scope of work

- Public API: `jit(source, CompileOptions)`, `jit_from_ir(ir, options)`,
  `jit_from_ir_owned(ir, options)` returning a **`JitModule`** (not raw tuple).
- `compile.rs`: orchestrate GLSL → Naga → LPIR (+ metadata) → Cranelift JIT;
  owned path sorts functions by size and drops each `IrFunction` after define.
- **`lpir`:** `GlslModuleMeta` / per-function metadata (param qualifiers, GLSL types).
- **`lp-glsl-naga`:** extend `FunctionInfo`, change `lower()` return type, wrap
  lowering errors with **function name**.
- **`lpir-cranelift`:** `values.rs` (`GlslQ32`, `GlslReturn`, `CallError`),
  Level 1 `call()`, Level 3 `direct_call()` via Rust trampoline / `lp-glsl-jit-util`.
- **Errors:** function name on lowering + emission; Naga parse keeps line-oriented
  strings; no per-op spans in Stage IV.

## File structure

```
lp-glsl/lpir/src/
├── glsl_metadata.rs              # NEW: GlslParamQualifier, GlslParamMeta,
│                                 #   GlslFunctionMeta, GlslModuleMeta, GlslTypeKind
├── lib.rs                        # UPDATE: pub use glsl_metadata::*
└── ...

lp-glsl/lp-glsl-naga/src/
├── lib.rs                        # UPDATE: FunctionInfo + param qualifiers
├── lower.rs                      # UPDATE: return (IrModule, GlslModuleMeta);
│                                 #   map_err with function name
├── lower_error.rs                # UPDATE: optional InFunction { name, inner }
└── ...

lp-glsl/lpir-cranelift/
├── Cargo.toml                    # UPDATE: naga, lp-glsl-naga, lp-glsl-jit-util deps
└── src/
    ├── lib.rs                    # UPDATE: public jit(), re-exports
    ├── compile.rs                # NEW: full pipeline, owned drain path
    ├── jit_module.rs             # UPDATE: JitModule struct, internal helpers
    ├── module.rs                 # NEW (optional): JitModule fields grouped
    ├── values.rs                 # NEW: GlslQ32, GlslF32, GlslReturn, CallError
    ├── direct_call.rs            # NEW: DirectCall, trampoline glue
    ├── error.rs                  # UPDATE: unified errors, parse/lowering/codegen
    ├── builtins.rs               # unchanged surface
    ├── q32.rs                    # unchanged (maybe pub(crate) encode for values)
    └── emit/                     # unchanged
```

## Conceptual architecture

```
                    jit(source, CompileOptions)
                    ═══════════════════════════
                              │
         ┌────────────────────┼────────────────────┐
         ▼                    ▼                    ▼
    Naga parse           lower()              jit_from_ir_owned
    (line-oriented       (IrModule,           (sort, define, drop)
     diagnostics)        GlslModuleMeta)
                              │
                              ▼
                    ┌──────────────────┐
                    │    JitModule     │
                    │  • JITModule     │
                    │  • FuncId map    │
                    │  • GlslModuleMeta│
                    │  • Signatures    │
                    │  • call_conv,    │
                    │    pointer_type  │
                    └────────┬─────────┘
                             │
              ┌──────────────┴──────────────┐
              ▼                             ▼
        Level 1: call()              Level 3: direct_call()
        GlslQ32 in/out               flat u32 buffers +
        metadata-driven              lp-glsl-jit-util /
        scalarize, Q32 encode        Rust trampoline
```

## Main components and how they interact

### `lpir::glsl_metadata`

- **`GlslParamQualifier`:** `In`, `Out`, `InOut`.
- **`GlslParamMeta`:** name, logical `GlslType` (reuse or mirror naga-facing enum —
  may duplicate `lp_glsl_naga::GlslType` or move shared enum to `lpir`; plan:
  **duplicate minimal surface in `lpir`** to avoid `lpir` depending on naga, or
  use `String` + kind — simplest is copy the `GlslType` enum into `lpir` as
  `GlslType` for metadata only, filled from naga lowering).
- **`GlslFunctionMeta`:** name, params, return type.
- **`GlslModuleMeta`:** `Vec<GlslFunctionMeta>` in stable order matching
  `IrModule::functions` after lowering (document ordering contract).

### `lp-glsl-naga`

- Extend **`function_info`** / **`FunctionInfo`** with qualifier per argument
  (from Naga `FunctionArgument` binding / pointer space).
- **`lower` → `Result<(IrModule, GlslModuleMeta), LowerError>`** — build metadata
  alongside `IrFunction` list; same iteration order as today.
- **`LowerError`:** add `InFunction { name: String, source: Box<LowerError> }` or
  prefix in `Display` only — prefer structured variant for tests.

### `lpir-cranelift::JitModule`

- Holds finalized `JITModule`, `HashMap<String, FuncId>` (or ordered vec + names),
  `GlslModuleMeta`, Cranelift `Signature` per function, `call_conv`,
  `pointer_type`.
- **`call(name, &[GlslQ32])`:** look up `GlslFunctionMeta`, flatten args to scalars,
  encode Q32, allocate stack for out/inout if needed, invoke native (or
  struct-return helper), decode results into `GlslReturn<GlslQ32>`.
- **`direct_call(name)`:** returns `Option<DirectCall>` with function pointer +
  trampoline calling convention.

### `compile.rs`

- **`jit`:** `compile` → `lower` → `jit_from_ir_owned` with `CompileOptions`.
- Maps **`LowerError`**, **`CompileError`**, Naga parse into unified **`CompilerError`**
  (or extend `CompileError` variants: `Parse`, `Lower`, `Codegen`).

### Error policy

- Parse: existing Naga string (includes lines).
- Lower: **`InFunction`** or prefixed message with function name.
- Codegen: every path includes **`IrFunction.name`**.
