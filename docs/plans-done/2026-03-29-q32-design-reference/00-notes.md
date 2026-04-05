# Plan notes — Q32 design doc + reference implementation

## Scope of work

- Add a single design document at `docs/design/q32.md` that describes:
    - Q16.16 encoding, range, and conversion rules (`from_f32`, `from_i32`, saturating/clamped
      paths).
    - Arithmetic semantics: saturating add/sub/mul/div, div-by-zero, remainder, overflow.
    - How Q32 relates to LPIR's float-mode-agnostic ops and to each backend (Cranelift JIT Q32, WASM
      preview if applicable).
    - **Relational / builtin edge policy**: `isnan`, `isinf`, comparisons vs IEEE, and what is *
      *`@unsupported(float_mode=q32, …)`** vs implemented behavior.
    - Named constants (`Q32::PI`, etc.) and their intended numeric meaning.
- Bring the **reference implementation** (`lp-glsl-builtins` `Q32` and closely related helpers) in
  line with that document:
    - Make `Q32` operators saturating (matching JIT builtins).
    - Fix `Q32::div` to match agreed div-by-zero semantics.
    - Every public API on `Q32` (and traits in the same module) should have **tests** that lock
      behavior (including edge cases called out in the doc).
    - Where JIT/LPIR behavior must match, cross-reference the doc from code comments sparingly.
- Audit and fix **filetests** that assert Q32 edge behavior, and add new ones where coverage is
  missing.
- Audit **JIT `extern "C"` builtins** (`__lp_lpir_f*_q32`) for agreement with the design doc.

Out of scope: rewriting compiler lowering (that remains roadmap work; this plan **feeds** it).
WASM emitter audit is deferred to a separate task.

## Current state of the codebase

### Documentation (fragmented)

| Location                                                                                   | What it says                                                                                                                  |
|--------------------------------------------------------------------------------------------|-------------------------------------------------------------------------------------------------------------------------------|
| `docs/lpir/00-overview.md`                                                                 | LPIR is float-mode-agnostic; Q32 is applied in emitters; numeric table is IEEE-oriented for abstract float.                   |
| `docs/plans-done/2026-03-11-direct-q32-part-c/06-inline-q32-builtins.md`                   | Q32 `isnan` → always false; Q32 `isinf` → compares to `0x7FFF_FFFF` and `i32::MIN` (div-by-zero saturation sentinels).        |
| `docs/roadmaps/2026-03-29-lpir-parity/milestone-i-relational-expressions.md`               | Product stance: **`isnan` / `isinf` on Q32 always false**; aligns with no NaN/Inf encoding and `@unsupported` for edge tests. |
| `docs/plans/2026-03-29-lpir-feature-parity/07-phase-q32-edge-unsupported.md` (and similar) | Filetests that need real IEEE NaN/Inf mark `@unsupported(float_mode=q32, …)`.                                                 |

There is **no** `docs/design/` file today; Q32 behavior is inferred from the above plus emitter
code.

### Reference type: `lp-glsl-builtins` `Q32`

File: `lp-shader/lp-glsl-builtins/src/glsl/q32/types/q32.rs`

- Q16.16: `SHIFT = 16`, raw `i32` payload.
- **Public surface**: constants, `from_fixed` / `from_f32` / `from_i32`, `to_f32` / `to_fixed`,
  `clamp`, `min`/`max`, `abs`, `is_zero`, `frac`, `to_i32`, `to_u8_clamped`, `to_u16_clamped`,
  `mul_int`, `mix`, `sqrt`; `Add`/`Sub`/`Mul`/`Div`/`Rem`/`Neg` and `*Assign`; `ToQ32` /
  `ToQ32Clamped`.
- **Edge behavior already in code**: `Div` and `Rem` by zero → `Q32(0)` (not trapping). Add/Sub
  wrap. Mul truncates (no saturation).
- **`#[cfg(test)]` module**: covers constants, conversions, arithmetic, assign ops, clamp/min/max,
  `to_q32*` / `to_q32_clamped`, `to_u8_clamped`. **Gaps** (no dedicated tests observed):
  `from_fixed`, `abs`, `is_zero`, `frac`, `to_i32`, `to_u16_clamped`, `mul_int`, `mix` (may live in
  `fns/mix.rs`), `sqrt`, `%` / `Rem`, div-by-zero and mul overflow/wrapping, verification of `PI`/
  `TAU`/`E`/`PHI` against known float values.
- Comment/naming: constant block has **mismatched comments** (e.g. `E` labeled as 2π in comment;
  `PHI` labeled as e). The design doc should state the **intended** values; fixing
  comments/constants is part of this plan.

### JIT `extern "C"` builtins

Files: `lp-shader/lp-glsl-builtins/src/builtins/lpir/{fadd,fsub,fmul,fdiv,fsqrt,fnearest}_q32.rs`

- `fadd`, `fsub`, `fmul`: all saturate via i64 widening + clamp. Tests cover basic ops, signs,
  overflow/underflow.
- `fdiv`: saturates with sign on div-by-zero (`pos/0 → MAX_FIXED`, `neg/0 → MIN_FIXED`), saturates
  on overflow. Tests cover basic, signs, div-by-zero, edge, small divisors.
- `fsqrt`: returns 0 for negative/zero input. Tests cover perfect/non-perfect squares, edge cases,
  large values.
- `fnearest`: roundeven semantics. Tests cover halfway even/odd, negative, normal.
- **`Q32` struct disagrees** with these builtins on overflow behavior (wrapping vs saturating).

### Filetests

- `common-isinf.glsl`: expects IEEE behavior (`isinf(1.0/0.0) == true`), no Q32-specific
  expectations, first test `@unimplemented(backend=jit)`.
- `common-isnan.glsl`: expects IEEE, first test `@unimplemented(backend=jit)`, rest expect false for
  normal values.
- `edge-nan-inf-propagation.glsl`: file-level `@unsupported(float_mode=q32)` — correctly skipped.
- `op-divide.glsl` (float): only normal cases, no edge/div-by-zero tests.
- No filetests for Q32 div-by-zero, overflow, or saturation behavior.

### Compiler / runtime paths (for cross-references in the doc)

- Naga → LPIR lowering (`lp-glsl-naga`): relational `isnan`/`isinf` behavior has been **mixed** (
  IEEE-style vs sentinel-style) relative to written docs — to be aligned after the normative Q32
  section exists.
- `lower_expr.rs` has `Q32_DIV0_POS`/`Q32_DIV0_NEG` sentinel constants used for `isinf` lowering —
  contradicts "always false" decision; needs cleanup in roadmap work.

## Questions (iterate with the user)

### Q1 — Normative `isnan` / `isinf` on Q32 (blocks design doc + lowering alignment)

**Context:** Two written policies exist: (A) `isnan` always false, `isinf` sentinel comparison to
max/min fixed patterns; (B) milestone I — **both always false** for Q32, with edge cases covered by
`@unsupported(float_mode=q32, …)` instead of pretending Q32 is IEEE.

**Answer:** Adopted **(B)**. Both always false. Sentinel bit patterns from div-by-zero saturation
are not surfaced through `isinf`.

### Q2 — Scope of "reference implementation"

**Context:** Q32 appears in `lp-glsl-builtins`, Q32 builtins under `glsl/q32/fns/`, LPIR interpreter
tests, and `lp-shader/lpir-cranelift`.

**Answer:** Primary reference = `Q32` struct + `fns/` + JIT `extern "C"` builtins + filetests. All
must agree. Design doc is the single source of truth; filetests are the executable proof.

### Q3 — WASM numeric mode

**Context:** LPIR overview mentions WASM may use `i64` paths; Q32 JIT may differ.

**Answer:** Design doc gets a short "Backend conformance" section: all backends must agree with the
Q32 spec. WASM implementation details are left to `lp-glsl-wasm` crate docs. This plan does not
audit the WASM emitter.

## References

- `docs/lpir/00-overview.md` — IR vs emitter split, numeric summary table.
- `docs/plans-done/2026-03-11-direct-q32-part-c/06-inline-q32-builtins.md` — historical Q32 builtin
  expansions.
- `docs/roadmaps/2026-03-29-lpir-parity/milestone-i-relational-expressions.md` — milestone I
  relational / `isnan`/`isinf` stance.
- `docs/plans/2026-03-29-lpir-parity-stage-i/00-notes.md` — related implementation plan notes.

## Notes

### Q1 answer — `isnan` / `isinf` on Q32

Adopted: both always false. Q32 has no NaN or Inf encoding. Sentinel bit patterns from
div-by-zero saturation are **not** surfaced through `isinf`; they are an implementation detail.

### Div-by-zero on Q32 (raised during Q1 discussion)

Three layers had three different behaviors — now normalized to:

- `0 / 0` → `Q32(0)` (no meaningful sign; "NaN-like" → zero)
- `pos / 0` → `Q32(0x7FFF_FFFF)` (max positive; sign-preserving saturation)
- `neg / 0` → `Q32(i32::MIN)` (max negative; sign-preserving saturation)

This matches `__lp_lpir_fdiv_q32` for the nonzero cases. The reference `Q32::div` in
`lp-glsl-builtins` needs to be updated from "always zero" to this behavior.

`Rem` by zero: stays as `Q32(0)` (GLSL `mod(x, 0.0)` is undefined; zero is safe).

### Overflow / saturation strategy (raised during Q1/div-by-zero discussion)

Adopted **Option B: single `Q32` type, saturating by default**.

- `Q32` operators (`+`, `-`, `*`, `/`) all saturate to `[i32::MIN, 0x7FFF_FFFF]`, matching
  the JIT `extern "C"` builtins (`__lp_lpir_fadd_q32`, etc.).
- The `.0` field remains `pub` for rare cases needing raw `i32` access.
- The `extern "C"` builtins keep their own saturation logic (they already don't use the struct).
- No second type. The design doc says "Q32 saturates" and the code matches everywhere.

Rejected alternatives:

- **Two types** (`Q32` + `Q32Raw`): adds confusion about which to use; the `extern "C"` builtins
  already operate on raw `i32` for the hot JIT path.
- **Wrapping by default + explicit saturation methods**: makes the wrong behavior the easy path;
  every new builtin becomes a potential overflow bug on MCU.
