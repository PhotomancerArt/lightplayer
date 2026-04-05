# LPIR Feature Parity — Notes

**Design:** [00-design.md](./00-design.md) · **Phases:
** [01](./01-phase-relational-bvec.md) … [09](./09-phase-cleanup-validation.md)

## Scope of work

Bring the Naga → LPIR → Cranelift pipeline to **GLSL feature parity** with the filetest corpus.
The legacy compiler (`lps-cranelift`, `lps-frontend`) has already been removed from the
dependency graph. This plan addresses the remaining **language coverage gaps** surfaced by the
filetest suite.

**Baseline (2026-03-29 jit.q32):** 501/651 files pass (77%), 150 fail. Failures cluster into
six root causes documented in the
[feature parity audit](../../reports/2026-03-29-lpir-feature-parity-audit.md).

**Target:** All 651 filetest files pass on `jit.q32` (excluding cases marked `@unsupported` for
inherent Q32 limitations like NaN/Inf semantics). WASM backend parity is **in scope** — LPIR
exists to share one lowering across backends, so features added here must work on both Cranelift
and WASM. WASM-specific `@unimplemented` annotations for pre-existing gaps are a separate concern.

## Current state

### Working

- Scalars (`float`, `int`, `uint`, `bool`), vectors (`vec2`–`vec4`, `ivec*`, `uvec*`), swizzles,
  arithmetic, comparisons, control flow (`if`/`else`, `for`, `while`, `do-while`, `switch`,
  `break`/`continue`/`return`), user functions with `in`/`out`/`inout` params, LPFX builtins,
  transcendental math via `@glsl` imports — all pass filetests and work on ESP32.

### Not working (by root cause, largest first)

1. **Matrix types** (~55 files) — `GlslType` metadata, `compile()` signatures,
   `naga_type_inner_to_glsl`, host invoke for >4-word returns, matrix element stores, matrix
   builtins (`transpose`, `inverse`, `determinant`, `outerProduct`, `matrixCompMult`).

2. **Relational expressions** (~29 files) — `Expression::Relational { All, Any, Not, IsNan,
   IsInf }` not handled in `lower_expr.rs`. Blocks all bvec aggregate operations and vector `==`.

3. **Vector comparison / harness** (~15 files) — some depend on #2; some show discrepancies
   between individual and full-suite runs (harness state issue).

4. **Arrays + structs** (~6 unexpected-fail files, many more `@unimplemented`) —
   `naga_type_to_ir_types` rejects `Array` and `Struct`.

5. **Const evaluation / diagnostics** (~15 files) — mixed: some matrix-blocked (resolved by #1),
   some diagnostic code mismatches, some const-folding edge cases.

6. **Minor / isolated** (~4 files) — `while (bool j = expr)` condition declarations, postfix
   component inc/dec semantics, one global init ordering issue.

### Key code touchpoints

- `lp-shader/lps-naga/src/lower_expr.rs` — expression lowering (relational, matrix, array)
- `lp-shader/lps-naga/src/lower_stmt.rs` — statement lowering (matrix stores, aggregates)
- `lp-shader/lps-naga/src/lower_ctx.rs` — `naga_type_to_ir_types` (type mapping)
- `lp-shader/lps-naga/src/lower_math.rs` — math builtin decomposition
- `lp-shader/lps-naga/src/lib.rs` — `naga_type_inner_to_glsl`, `extract_functions`
- `lp-shader/lpir/src/glsl_metadata.rs` — `GlslType` enum
- `lp-shader/legacy/lpir-cranelift/src/invoke.rs` — host JIT calling, return decode
- `lp-shader/lps-filetests/` — test harness and `.glsl` corpus

## Questions

### Q1 — Scope: arrays and structs in this plan or deferred?

**Context:** Arrays and structs require significant IR surface area (slot layout, element
addressing, member access, ABI for aggregate params/returns). Only 6 files fail with unexpected
errors today; many more are annotated `@unimplemented`. Current product shaders don't use arrays
or structs. Matrices are a much higher-impact gap.

**Suggested answer:** Defer arrays and structs to a follow-up plan. Focus this plan on matrices,
relational expressions, diagnostics, and polish — the items that bring coverage from 77% to ~90%+
and cover realistic product shader needs.

**Answer:** Defer arrays and structs to a follow-up plan.

---

### Q2 — Matrix return strategy: LPIR vs backend invoke

**Context:** Matrix returns involve two separate layers:

- **LPIR layer:** A `mat3`-returning function is just a 9-value `f32` return in LPIR. This is a
  lowering/metadata concern — `GlslType`, `naga_type_inner_to_glsl`, `extract_functions` need to
  accept matrices and flatten them to scalarized VRegs. The IR, interpreter, and WASM backend
  should handle multi-value returns naturally once the lowering is correct.

- **Host invoke layer:** `invoke_i32_args_returns` in `lpir-cranelift/src/invoke.rs` caps at 4
  return words today. For `mat3` (9 words) and `mat4` (16 words), the host glue needs to use
  Cranelift's `enable_multi_ret_implicit_sret` (caller-allocated return area). This is a
  Cranelift-specific concern, not an LPIR concern.

**Suggested answer:** Separate these into two plan phases. First phase: get LPIR lowering and
metadata right for matrices (works for interpreter and WASM immediately). Second phase: fix host
invoke glue for sret, unblocking Cranelift JIT tests with large matrix returns.

**Answer:** Agreed — two phases. LPIR lowering first (metadata, signatures, scalarized returns),
then Cranelift invoke glue (sret for >4-word returns) as a separate phase.

---

### Q3 — Diagnostic fidelity: fix or annotate?

**Context:** 4 `type_errors/` files fail because the LPIR path produces different error codes than
expected (e.g. `E0400 unsupported expression` instead of `E0112 post-increment requires numeric
operand`). Fixing this requires adding pre-lowering validation in `lps-naga` to catch these
cases before they reach the general lowering error path.

**Suggested answer:** Fix the diagnostics — they're small targeted checks (e.g. "reject `++` on
bool before attempting to lower the binary Add") and improve the user-facing error quality. Don't
just annotate the tests as `@broken`.

**Answer:** Fix diagnostics (pre-lowering validation in `lps-naga`).

---

### Q4 — Q32 edge-case builtins: fix or annotate?

**Context:** `builtins/edge-{trig-domain,exp-domain,nan-inf-propagation}` test IEEE float edge
behavior (NaN, Inf, extreme domain values) that Q32 fixed-point math fundamentally cannot
represent. These are **inherent Q32 limitations**, not LPIR bugs.

**Suggested answer:** Use `@unsupported(float_mode=q32, reason="…")` — semantically clear that
the feature (IEEE NaN/Inf semantics) is **not applicable** on Q32, not a temporary bug (`@broken`)
or unfinished work (`@unimplemented`).

**Answer:** Mark with `@unsupported(float_mode=q32, reason="…")` (not `@broken`).

---

### Q5 — Filetest harness investigation: in scope or separate?

**Context:** Some files (e.g. `uvec2/fn-equal.glsl`) pass when run individually but show as
failed in the full suite. The single-threaded run produces nearly identical results (150 files
either way), so this may be a test-counting or global-state issue rather than a threading bug.
The discrepancy affects ~5-10 files at most.

**Suggested answer:** Include a small investigation step in the final polish phase. If it turns
out to be a quick fix (e.g. stale global state between test files), fix it. If it requires major
harness refactoring, file it as a separate follow-up.

**Answer:** Yes — investigate in the final polish phase; fix if straightforward, otherwise spin off
a follow-up.

## Notes

- **Filetest annotations:** `@ignore` was renamed to **`@unsupported`** in the harness (2026-03-29).
  Meaning: not applicable on the filtered target **by design** (permanent), vs `@unimplemented`
  (temporary gap) vs `@broken` (known bug / wrong expectation until fixed).
- **WASM + Cranelift parity:** Features in this plan must be verified on both backends; LPIR is
  the shared lowering layer.
