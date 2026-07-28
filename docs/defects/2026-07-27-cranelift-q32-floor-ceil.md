---
status: fixed
found: 2026-07-27      # how: new intrin_* torture axis, first run
fixed: this change
area: lp-shader/lpvm-cranelift (Q32 emission, rv32c.q32)
class: untested-path
related:
  - docs/defects/2026-07-27-wasm-q32-fabs-stack-leak.md
---
# Cranelift Q32 `floor` / `ceil` were wrong for runtime negative values

Sibling of the `trunc` fallout recorded in
[wasm-q32-fabs-stack-leak](2026-07-27-wasm-q32-fabs-stack-leak.md) (PR #155).
That change fixed `emit_ftrunc`; `emit_ffloor` and `emit_fceil` carried the same
mistaken premise and were left behind. This change fixes those two.

**Symptom** — On `rv32c.q32` only, with a **runtime** (non-constant) argument:

| call | expected | rv32c gave |
| --- | --- | --- |
| `floor(-0.6875)` | `-1.0` | `-2.0` |
| `ceil(-0.6875)` | `0.0` | `-1.0` |

`fract` and `mod` lower through `floor`, so they were wrong too. 23 directives
across `intrin_floor`, `intrin_ceil`, `intrin_fract` and `intrin_mod` failed on
`rv32c.q32` against post-#155 main while all four other targets agreed.

**Root cause** — the same premise error as `trunc`: `band(v, !Q32_FRAC)` was
treated as truncation toward zero, but clearing the low bits of a
**two's-complement** Q16.16 value already rounds toward -∞. `-0.6875` is
`0xFFFF_5000`, which masks to `0xFFFF_0000` = `-1.0`.

- `emit_ffloor` then subtracted a further 1.0 for negative values with a
  fractional part — double-counting, hence `-2.0`.
- `emit_fceil` added 1.0 only when `v >= 0`, so `ceil` of a negative
  non-integer just returned `floor(v)`.

**Fix** — a shared `emit_mask_to_floor` helper now states in one place that the
mask *is* `floor`; `ceil` and `trunc` derive from it (`ceil = floor + has_frac`,
`trunc = floor + (has_frac && negative)`), matching
`lpvm_wasm::emit::q32::emit_q32_ffloor` / `emit_q32_ftrunc`.

**Regression coverage** — `control/torture/intrin_floor.glsl`, `intrin_ceil`,
`intrin_trunc`, `intrin_fract`, `intrin_mod`, each driving the builtin from a
loop-derived runtime value.

**Standing signal (the reason both halves of this hid)** — every case in
`filetests/builtins/common-floor.glsl` and its ceil/trunc/fract siblings passes a
**literal** (`floor(-2.3)`, `floor(-0.1)`). Those fold at compile time and never
reach a backend emitter, so that suite has been testing constant folding rather
than codegen. When adding builtin coverage, take at least one argument from a
function parameter or loop variable.
