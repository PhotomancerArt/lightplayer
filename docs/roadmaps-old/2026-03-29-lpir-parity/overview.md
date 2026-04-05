# LPIR Parity Roadmap — Overview

## Motivation

The Naga → LPIR → (Cranelift | WASM) pipeline replaced the legacy `lps-cranelift` compiler.
It is structurally complete, validated on ESP32-C6, and works for product shaders. However, 50 of
651 filetest files still fail on `jit.q32` due to missing GLSL feature coverage — concentrated in
relational expressions on bvec, matrix element stores, bvec casts/dynamic indexing, and arrays.

This roadmap closes those gaps (excluding structs) so the filetest suite exits clean on all
backends.

## Done bar

`./scripts/glsl-filetests.sh` exits **0** on `jit.q32`: zero unexpected failures. Every file
either passes or is annotated `@unimplemented` (structs, Naga parse limitations) or `@unsupported`
(Q32 edge semantics). Same standard for WASM and RV32 after the multi-backend milestone.

## Baseline marking (optional, before Milestone I)

To keep **each milestone starting from a green suite** (only *new* failures are visible), you can
pre-mark every current failure as an expected gap:

1. Run the filetests app with **exactly one** target, e.g. `jit.q32` (default if you omit
   `--target`, since `DEFAULT_TARGETS` is JIT-only).
2. Use `--mark-unimplemented` (or `LP_MARK_UNIMPLEMENTED=1`). You will be prompted to type `yes`
   unless you pass `--assume-yes`.
3. The runner adds `// @unimplemented(backend=jit)` **before the first `// run:`** when the whole
   module fails to compile in **summary** mode (the usual multi-file run), or *
   *`// @unimplemented(backend=jit)`**
   immediately before each failing `// run:`** when the file compiled but individual directives
   failed (or in single-file **detail** mode, where there is no whole-file compile step).
4. Re-run the suite; exit code should be **0** with failures counted as expected `@unimplemented`.
5. While implementing a milestone, remove annotations only for the tests you are fixing; use
   `LP_FIX_XFAIL=1` / `--fix` to strip markers from tests that now pass.

Requires: `cargo run -p lps-filetests-app -- test --target jit.q32 --mark-unimplemented`
(or equivalent via `scripts/glsl-filetests.sh` with those flags in the argument list).

## Architecture

No new crates. Work is within existing crates:

```
lp-shader/
├── lps-naga/src/
│   ├── lower_expr.rs              # Relational, bvec casts, dynamic index
│   ├── lower_stmt.rs              # Matrix/bvec element stores
│   ├── expr_scalar.rs             # Type inference for Relational
│   └── lib.rs                     # extract_functions, metadata
├── lpir/src/
│   └── glsl_metadata.rs           # GlslType (matrix variants from WIP)
├── lpir-cranelift/src/
│   └── invoke.rs                  # sret for large returns
├── lps-wasm/                  # Verify multi-return emit
├── lps-filetests/
│   ├── src/                       # `--mark-unimplemented`, multi-target report (later)
│   └── filetests/                 # Annotation cleanup
└── lps-filetests-app/         # CLI: `--fix`, `--mark-unimplemented`, `--assume-yes`
```

## Alternatives considered

- **Modify Naga fork for parse gaps** — Rejected. Minimize fork surface; annotate or rewrite
  tests instead. `matrixCompMult`, `mix(bvec)`, `while (bool j = expr)` marked
  `@unimplemented(reason="Naga frontend limitation")`.
- **Include structs** — Rejected. Only 2 failing files, high IR cost (layout, member access,
  aggregate ABI), not used in product shaders. Deferred to a separate roadmap.
- **Interleave WASM parity with each milestone** — Rejected. Adds scope to each focused milestone.
  Better as a final sweep with proper comparison tooling.

## Risks

- Matrix invoke (sret) may surface per-platform ABI differences (AArch64 / x86 / RV32).
- Array lowering (slot layout, element addressing) is the least-explored area and may uncover
  LPIR design gaps in pointer/offset representation.
- Naga frontend limitations may expand as more GLSL surface area is exercised.

## Milestones

| #      | Focus                                                                    | Files unblocked         | Primary crate          |
|--------|--------------------------------------------------------------------------|-------------------------|------------------------|
| (prep) | Baseline `@unimplemented(backend=…)` on all current failures (see above) | suite green             | `lps-filetests`    |
| I      | Relational expressions (`all`/`any`/`not`, matrix `==`, `isnan`/`isinf`) | ~21                     | `lps-naga`         |
| II     | Pointer stores/loads (matrix element, bvec dynamic index)                | ~15                     | `lps-naga`         |
| III    | Bvec lowering gaps (casts, `mix`, misc)                                  | ~6                      | `lps-naga`         |
| IV     | Array type lowering                                                      | ~5+                     | `lps-naga`, `lpir` |
| V      | Matrix invoke / sret (large returns)                                     | unlocks mat3/mat4 tests | `lpir-cranelift`       |
| VI     | Multi-backend parity (WASM/RV32 sweep + comparison tooling)              | cross-target            | `lps-filetests`    |
| VII    | Annotations, polish, closure                                             | remaining edge cases    | all                    |
