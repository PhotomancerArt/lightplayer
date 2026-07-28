---
status: fixed
found: 2026-07-27      # how: new intrin_* torture axis, first run
fixed: this change
area: lp-shader/lpvm-cranelift (Q32 emission, rv32c.q32)
class: untested-path
related:
  - docs/defects/2026-07-27-wasm-fabs-leaks-operand.md
---
# Cranelift Q32 `floor` / `ceil` / `trunc` were wrong for runtime negative values

**Symptom** — On `rv32c.q32` only, `floor(-0.6875)` returned `-2.0` instead of
`-1.0`. Surfaced immediately when the new `control/torture/intrin_*` axis first
ran: `intrin_floor`, `intrin_ceil`, `intrin_trunc`, `intrin_fract` and
`intrin_mod` disagreed with all four other targets (`fract`/`mod` lower through
`floor`). Independent of loops or branches — a plain
`float f(float x) { return floor(x); }` reproduced it.

**Root cause** — `q32_emit::emit_ffloor` treated `band(v, !Q32_FRAC)` as a
truncation toward zero and then subtracted 1 for negative values with a
fractional part. But clearing the low bits of a **two's-complement** Q16.16 value
already rounds toward -∞: `-0.6875` is `0xFFFF_5000`, which masks to
`0xFFFF_0000` = `-1.0`. The extra adjustment double-counted, giving `-2.0`.

The same mistaken premise ran through the neighbours:

- `emit_fceil` added 1 only when `v >= 0`, so `ceil` of a negative non-integer
  returned `floor(v)` (`ceil(-0.6875)` → `-1.0`, should be `0.0`).
- `emit_ftrunc` was the bare mask, i.e. `floor`, so `trunc(-0.6875)` → `-1.0`
  instead of `0.0`.

The WASM and lpvm-native emitters were correct throughout, so this was a
single-backend divergence.

**Why it hid for so long** — every case in `filetests/builtins/common-floor.glsl`
(and its ceil/trunc/fract siblings) passes a **literal**: `floor(-2.3)`,
`floor(-0.1)`. Those fold at compile time and never reach the backend emitter, so
the suite exercised constant folding rather than codegen. No existing test called
these builtins with a runtime value on `rv32c.q32`.

**Fix** — a shared `emit_mask_to_floor` helper documents that the mask *is*
`floor`; `ceil` and `trunc` are now derived from it (`ceil = floor + has_frac`,
`trunc = floor + (has_frac && negative)`), matching
`lpvm_wasm::emit::q32::emit_q32_ffloor` / `emit_q32_ftrunc`.

**Regression coverage** — `filetests/control/torture/intrin_floor.glsl`,
`intrin_ceil`, `intrin_trunc`, `intrin_fract`, `intrin_mod`. All five drive the
builtin from a loop-derived runtime value, so the constant-folding escape hatch
does not apply.

**Standing signal** — a builtin filetest that only uses literal arguments tests
the frontend, not the backend. When adding builtin coverage, take at least one
argument from a function parameter or loop variable.
