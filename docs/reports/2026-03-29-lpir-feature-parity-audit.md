# LPIR Feature Parity Audit

**Date:** 2026-03-29
**Branch:** `feature/lpir-cranelift`
**Prior reports:** [post-refactor audit](2026-03-26-lp-glsl-post-refactor-audit.md), [Stage VI-C validation](2026-03-26-lpir-cranelift-vi-c-ab.md)
**Prior gap analysis:** [`docs/roadmaps/2026-03-25-lpir-features/todo.md`](../roadmaps/2026-03-25-lpir-features/todo.md)

## Purpose

Take stock of the Naga → LPIR → Cranelift pipeline's feature completeness relative to the GLSL
surface exercised by filetests and product shaders. Identify remaining gaps and rank them for a
parity plan.

## What's done

The LPIR refactor is **structurally complete and validated on hardware**:

- **Pipeline wired end-to-end.** `lp-engine` → `lpir-cranelift` → JIT on host and ESP32-C6.
  `lp-glsl-wasm` → WASM for browser preview. `lpir::interp` for IR-level testing.
- **Legacy compiler removed.** No `Cargo.toml` in the workspace references `lp-glsl-cranelift`,
  `lp-glsl-frontend`, or `esp32-glsl-jit`. The old crates are out of the dependency graph.
- **Firmware validated.** `fw-tests` pass (scene render, alloc trace, unwind). `fw-esp32` builds
  and runs on device (Stage VI-C checklist).
- **Core crate tests pass.** `lpir` (168), `lpir-cranelift` (32), `lp-glsl-naga` (51),
  `lp-engine` (4), `lp-server` (4) — all green.
- **Documentation refreshed.** Per the 2026-03-26 post-refactor audit, READMEs, `CRATES.md`,
  `AGENTS.md`, `docs/architecture.md`, `docs/lpir/` spec, and `scripts/build-builtins.sh` were
  updated.
- **Product shaders compile and run.** The `rainbow.shader` example (float/vec/LPFX/trig/control
  flow) executes correctly through the LPIR path.

## Filetest results (jit.q32, 2026-03-29)

Single-threaded (`LP_FILETESTS_THREADS=1`) to rule out harness concurrency artifacts:

| Metric | Count |
|--------|-------|
| Total files | 651 |
| Files passing | 501 (77%) |
| Files failing | 150 (23%) |
| Total test cases | 2530 |
| Pass | 380 |
| Expected-failure (`@unimplemented`) | 944 |
| Unexpected failure | 1206 |

The 944 expected-failure cases are tests annotated `@unimplemented(backend=wasm)` or similar — they
document known gaps intentionally and do not represent regressions. The 1206 unexpected failures are
concentrated in 150 files that cluster into a small number of root causes (see below).

The pass rate among non-expected-failure tests is 380 / (380 + 1206) ≈ **24%** by test case count,
but **77%** by file count — failures are concentrated in specific feature areas, not spread across
the suite.

## Failure root-cause analysis

All 150 failing files map to **six root causes**. The counts overlap slightly (a file may contain
tests hitting more than one gap), but each file's primary blocker is categorized below.

### 1. Matrix type not supported in LPIR metadata / lowering (~55 files)

**Error:** `unsupported type: Matrix { columns: …, rows: …, scalar: … }`

All 51 files under `matrix/mat2/`, `matrix/mat3/`, `matrix/mat4/` fail, plus
`operators/incdec-matrix-{column,element}.glsl`, `builtins/matrix-{compmult,inverse}.glsl`,
`function/return-matrix.glsl`, and several `const/` files whose failing test case returns `mat2`.

**Root cause:** `naga_type_inner_to_glsl` in `lp-glsl-naga/src/lib.rs` and
`naga_type_to_ir_types` in `lower_ctx.rs` reject `Matrix` at the module-metadata and type-lowering
level. `GlslType` in `lpir/src/glsl_metadata.rs` has no matrix variant. Even though internal
lowering of matrix locals to scalarized VRegs exists (and is exercised by some `lp-glsl-naga`
unit tests), the `compile()` → `extract_functions()` path rejects functions whose signatures
reference matrix types.

**Scope:** Metadata (`GlslType`), lowering (`naga_type_inner_to_glsl`), invoke/ABI for host JIT
(matrix returns need >4-word decode), matrix element stores (`lower_stmt.rs` explicit rejection).

**Unlocks:** `matrix/**`, `operators/incdec-matrix-*`, `builtins/matrix-*`, `function/return-matrix`,
some `const/` and `function/forward-declare` files (which fail because the file contains
matrix-typed forward declarations that poison the whole compile).

### 2. Relational expressions on bvec: `all()`, `any()`, `not()` (~29 files)

**Error:** `unsupported expression: Relational { fun: All, argument: … }` (and `Any`, `Not`)

All `vec/bvec2/*`, `vec/bvec3/*`, `vec/bvec4/*` files fail. GLSL's `==` on vectors desugars
through Naga into `Relational { fun: All, argument: <component-wise-eq> }`, so `bvec` equality
tests also hit this path.

**Root cause:** `lower_expr.rs` does not handle `Expression::Relational`. These are bvec-aggregate
operations (`all`, `any`, `not`) that need component-wise decomposition in the lowering.

**Scope:** Add a `Relational` arm in `lower_expr.rs` that decomposes to `iand`/`ior`/`ieq` chains
on the scalarized bvec components.

**Unlocks:** `vec/bvec{2,3,4}/*` (29 files), and several vector equality/comparison files that
depend on `Relational::All` for their `==` operator lowering.

### 3. Vector comparison returning bvec / missing `equal()`-family on some types (~15 files)

Files under `uvec{2,3,4}/fn-{equal,less-than,less-equal,greater-than,greater-equal}.glsl` and
generated `vec/*/op-{equal,not-equal}.gen.glsl` files.

Some of these pass when run individually but fail in the full suite (8 tests expected vs 7 found).
This appears related to a **filetest harness issue** where certain files behave differently in the
full run. The concurrency bug noted in `todo.md` §6 may be partially responsible, though
single-threaded results are nearly identical (150 failing files in both modes). Investigation is
warranted but is lower priority than the language gaps.

For the subset that genuinely fails: these depend on `Relational` lowering (root cause 2) or
produce bvec results that the invoke path cannot decode.

### 4. Array and struct types in lowering (~6 files)

**Error:** `unsupported type for LPIR: Array { … }` / `unsupported type … Struct { … }`

`array/declare-explicit`, `array/phase/{1,2,4}`, `struct/define-{simple,vector}`, and
`function/param-out-array`.

**Root cause:** `naga_type_to_ir_types` only handles `Scalar`, `Vector`, `Matrix`; arrays and
structs fall through to the unsupported error. This blocks stack-slot layout, element addressing,
and struct member access.

**Scope:** IR representation (slot-based layout for aggregates), lowering in `lower_ctx.rs` /
`lower_expr.rs` / `lower_stmt.rs`, and ABI/metadata for functions with aggregate params or returns.

**Unlocks:** `array/**` (currently 4 unexpected-fail + many `@unimplemented`), `struct/**` (2
unexpected-fail + many `@unimplemented`), `const/array-size/*`.

### 5. Const evaluation and diagnostics (~15 files)

Several `const/` files fail for matrix reasons (root cause 1). The remaining `const/` failures are:

- `const/qualifier/{must-init,readonly,write-error}` — some test cases within these files fail
  because they reference matrix-typed globals.
- `const/expression/{constructors,literal}`, `const/errors/{non-const-init,user-func}` — these
  appear to fail because the Naga frontend evaluates const expressions differently than expected,
  or the LPIR path doesn't preserve const-ness for error reporting.

`type_errors/{incdec-bool,incdec-nested,incdec-non-lvalue,expected-error-line-offset}` — the LPIR
path emits different error codes than expected (e.g. `E0400` "unsupported expression" instead of
`E0112` "post-increment requires numeric operand"). These are diagnostic fidelity issues, not
codegen bugs.

### 6. Builtins edge cases (~6 files)

- `builtins/common-{isnan,isinf}` — `Relational { fun: IsNan/IsInf }` on vectors; same root
  cause as #2.
- `builtins/edge-{trig-domain,exp-domain,nan-inf-propagation}` — Q32 fixed-point math does not
  preserve IEEE NaN/Inf semantics. These tests exercise boundary behavior (e.g. `sin(1e30)`,
  `exp(-1e10)`) that Q32 cannot represent. These are **inherent Q32 limitations**, not bugs.
- `builtins/edge-precision` — 5/6 pass; 1 test fails on a tight tolerance.

### 7. Minor / isolated gaps (~4 files)

- `control/while/variable-scope` — 3/5 pass; failure is on `while (bool j = expr)` condition-
  declaration syntax (uncommon pattern, may be a Naga lowering edge case).
- `function/call-nested` — 5/8 pass; failing cases likely hit matrix-typed sub-expressions.
- `function/edge-out-not-read`, `function/param-unnamed`, `function/forward-declare` — these fail
  because the file contains forward declarations or parameters that reference matrix/array types
  (root cause 1 or 4), even though the specific test function being run does not use those types.
- `global/shared-multiple-init` — likely a global initialization ordering issue.

## Summary: feature gap size by root cause

| Root cause | Failing files | Test cases | Effort estimate |
|-----------|---------------|------------|-----------------|
| **Matrix type** (metadata + lowering + invoke) | ~55 | ~470 | Large (multi-stage) |
| **Relational exprs** (`all`/`any`/`not` on bvec) | ~29 | ~240 | Small-medium |
| **Vector comparison / harness** | ~15 | ~140 | Small + investigation |
| **Array + struct types** | ~6 | ~50 | Large (deferred) |
| **Const / diagnostics** | ~15 | ~40 | Medium (mixed causes) |
| **Builtins edge / Q32** | ~6 | ~40 | Small (isnan/isinf) + inherent limits |
| **Minor / isolated** | ~4 | ~20 | Small |

Note: file counts sum to >150 because some files are blocked by multiple root causes; the primary
blocker determines which bucket they land in.

## Product impact assessment

**Current product shaders work.** The `rainbow.shader` example and typical LED effect shaders use
`float`, `vec2`–`vec4`, arithmetic, trig, LPFX noise/color, `if/else`, `for` loops, and standard
builtins — all fully supported.

**Matrices** are the largest gap. Many shaders use `mat2`/`mat3` for 2D transforms; `mat4` is less
common in LED work but exists in the filetest corpus. The inability to compile any file that
*mentions* a matrix type (even in an unused forward declaration) makes this the highest priority.

**bvec `all()`/`any()`** affects shaders that use `==` on vectors, which is common enough to
matter.

**Arrays and structs** are used in more complex shaders (e.g. multi-light setups, palettes). These
are important for parity but less urgent for the typical product use case.

## Suggested parity plan (ordered)

### Phase 1: Quick wins (unblock ~30 files)

1. **Relational expression lowering** — handle `Expression::Relational { All, Any, Not, IsNan,
   IsInf }` in `lower_expr.rs`. Decompose to component-wise `iand`/`ior`/`ieq`/`feq`-with-NaN
   checks on scalarized bvec VRegs.
   - Unblocks: `vec/bvec*/*` (29 files), `builtins/common-isnan`, `builtins/common-isinf`.
   - Effort: ~1 session.

### Phase 2: Matrix support (unblock ~55 files)

2. **`GlslType` matrix variants** — add `Mat2`/`Mat3`/`Mat4` to `GlslType` in
   `lpir/src/glsl_metadata.rs` and `naga_type_inner_to_glsl` in `lp-glsl-naga/src/lib.rs`.
3. **Matrix in `compile()` signatures** — allow `extract_functions` to produce matrix-typed
   parameters and returns, flattened to scalarized VRegs.
4. **Host invoke for matrix returns** — extend `invoke_i32_args_returns` in
   `lpir-cranelift/src/invoke.rs` beyond 4-word cap. `mat2` = 4 words (fits today's limit),
   `mat3` = 9, `mat4` = 16 — needs stack return area or extended GPR decode.
5. **Matrix element stores** — lift the explicit rejection in `lower_stmt.rs` for
   `Store(AccessIndex(AccessIndex(…)))` on matrix locals.
6. **Matrix builtins** — wire `transpose`, `inverse`, `determinant`, `outerProduct`,
   `matrixCompMult` through the lowering (most already exist in `lower_math.rs` but may need
   metadata/invoke support to be end-to-end testable).
   - Unblocks: `matrix/**`, `operators/incdec-matrix-*`, `builtins/matrix-*`,
     `function/return-matrix`, several `const/` and `function/` files.
   - Effort: ~2-3 sessions.

### Phase 3: Diagnostics and const edge cases (~15 files)

7. **Diagnostic codes** — ensure `++` on bool produces `E0112` before reaching lowering. Check
   that const-init and const-qualifier errors match expected codes. May need pre-lowering
   validation in `lp-glsl-naga`.
8. **Const evaluation** — the Naga frontend handles most const folding; the remaining `const/`
   failures need case-by-case investigation (some may be matrix-related, resolved by Phase 2).
   - Effort: ~1 session.

### Phase 4: Arrays and structs (future, unblock ~6+ files)

9. **Array type lowering** — stack-slot layout, element addressing via `SlotAddr` + offset,
   bounds. Pairs with `Load`/`Store` through computed pointers.
10. **Struct type lowering** — member layout, `AccessIndex` on struct, nested structs.
11. **ABI for aggregate params/returns** — `out`/`inout` arrays and structs, stack-allocated
    return areas.
    - Unblocks: `array/**` (many currently `@unimplemented`), `struct/**`, `function/param-out-array`.
    - Effort: ~3-4 sessions (larger IR surface area).

### Phase 5: Polish

12. **Filetest harness investigation** — reproduce the "files pass individually but fail in suite"
    behavior; fix isolation or accounting.
13. **Q32 edge-case tests** — mark `builtins/edge-{trig,exp}-domain` and `edge-nan-inf-propagation`
    as `@unsupported(float_mode=q32, reason="…")` (Q32 lacks NaN/Inf; not applicable by design).
14. **Postfix inc/dec on vector components** — semantics bug noted in `todo.md` §5 (expected `3.0`,
    got `4.0` on `v.x++`). Likely a small fix in how Naga's postfix lowering is consumed.
15. **`while (bool j = expr)` condition declarations** — uncommon syntax; fix or annotate.

## Other workspace health

| Area | Status |
|------|--------|
| `lp-engine` tests | Pass (4/4) |
| `lp-server` tests | Pass (4/4) |
| `fw-tests` (emu) | Pass (scene_render, unwind) |
| `lpir` unit tests | Pass (168) |
| `lpir-cranelift` unit tests | Pass (32) |
| `lp-glsl-naga` unit + integration | Pass (51) |
| Legacy compiler references in Cargo.toml | **None** — fully removed |
| Documentation (READMEs, CRATES.md, lpir spec) | Up to date per 2026-03-26 audit |
| ESP32 binary size | 1,163,820 bytes (2026-03-26 measurement) |
| `scripts/build-builtins.sh` | Functional; hash paths fixed |

## Conclusion

The LPIR pipeline is **production-functional for current product shaders** and **structurally
complete** (legacy compiler removed, firmware validated, docs current). The remaining work is
**GLSL language coverage** — primarily matrix types (~55 files), bvec relational expressions (~29
files), and then arrays/structs for full parity. Phases 1–2 would bring the filetest pass rate
from 77% to ~90%+ of files; Phase 4 (arrays/structs) completes the long tail.
