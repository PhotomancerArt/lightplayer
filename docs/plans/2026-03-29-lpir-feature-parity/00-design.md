# LPIR feature parity — design

## Scope of work

Close GLSL coverage gaps on the **Naga → LPIR → (Cranelift | WASM | interp)** path so the
filetest corpus matches product expectations. **Arrays and structs** are **out of scope** — defer
to a follow-up plan.

**In scope**

- `Expression::Relational` (`all`, `any`, `not`, vector `isnan` / `isinf`) in `lps-frontend`.
- Matrix types in metadata and lowering (`GlslType`, `naga_type_inner_to_glsl`, signatures),
  matrix element stores, matrix builtins wired end-to-end.
- Host JIT **invoke** glue for multi-word / sret returns (`mat3` / `mat4`).
- **WASM** verification for the same features (LPIR is shared).
- **Diagnostics** for `type_errors/` expected codes.
- **`@unsupported(float_mode=q32, …)`** on IEEE edge tests that cannot apply to Q32.
- **Filetest harness** investigation (full suite vs single-file discrepancies).

**Reference:** [feature parity audit](../../reports/2026-03-29-lpir-feature-parity-audit.md),
[00-notes.md](./00-notes.md).

## File structure

```
lp-shader/
├── lps-frontend/src/
│   ├── lower_expr.rs              # UPDATE: Relational
│   ├── lower_stmt.rs              # UPDATE: matrix element stores
│   ├── lib.rs                     # UPDATE: matrix in module metadata / extract_functions
│   └── …
├── lpir/src/
│   └── glsl_metadata.rs         # UPDATE: GlslType mat2/3/4
├── lpir-cranelift/src/
│   ├── invoke.rs                  # UPDATE: sret / large returns
│   └── emit/                      # VERIFY: call ABI as needed
├── lps-wasm/                  # VERIFY / FIX: multi-return emission
├── lps-filetests/
│   ├── filetests/builtins/edge-*.glsl
│   └── src/                       # harness fixes if needed
└── lps-diagnostics/           # VERIFY: error codes (if touched from naga)
```

## Conceptual architecture

```
GLSL (#version 450) ──► Naga Module
                            │
                            ▼
                    lps-frontend (lower_expr / lower_stmt / …)
                            │
                            ▼
                    LPIR IrModule (scalarized vregs, multi-return)
                            │
            ┌───────────────┼───────────────┐
            ▼               ▼               ▼
      lpir::interp    lps-wasm     lpir-cranelift
      (reference)     (wasm-encoder)   (CLIF → machine)
                                            │
                                            ▼
                                    invoke.rs (host: read returns / sret)
```

**Separation:** LPIR expresses matrix returns as multiple scalar return values. Cranelift and WASM
each lower that to their ABI; **invoke.rs** is only the Rust-side caller for host JIT tests.

## Main components

| Component             | Role                                                                                                |
|-----------------------|-----------------------------------------------------------------------------------------------------|
| `lps-frontend`        | Maps Naga expressions/statements to LPIR; must accept matrix signatures and relational ops on bvec. |
| `lpir::glsl_metadata` | Describes exported function types for callers; must include matrix shapes for invoke/decode.        |
| `lpir-cranelift`      | Emits CLIF; host tests need invoke glue that matches Cranelift’s multi-return / sret ABI.           |
| `lps-wasm`            | Emits WASM; must remain consistent with LPIR multi-return for the same shaders.                     |
| `lps-filetests`       | Corpus + runner; annotations (`@unimplemented`, `@broken`, `@unsupported`).                         |
