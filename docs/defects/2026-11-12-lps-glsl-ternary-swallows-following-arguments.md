---
status: fixed
found: 2026-11-12      # how: lp-gfx-wgpu's interp differential, once its float oracle compiled through lps-glsl
fixed: 2026-11-12
area: lps-glsl (the device's GLSL front end) — expression parser
class: untested-path
related:
  - 2026-11-12-lps-glsl-has-no-bitwise-operators.md
---
# lps-glsl's ternary swallows the arguments after it

**Symptom** — `vec4(counter * 0.25, cond ? 1.0 : 0.0, 0.0, 1.0)` failed on a
board with "constructor for Vec4 expects 4 scalar lanes, got 2": the parser
read the ternary's last operand as a full expression, comma operator
included, so `0.0, 0.0, 1.0` became that operand and the call had two
arguments. A call whose arity still matched would have compiled with the
wrong values.

**Why it hid** — no corpus file put a ternary anywhere but last in an
argument list. naga, which the rest of the corpus compiled through, parses
it right.

**Fix** — the last operand parses as an assignment-expression, GLSL's
grammar (`logical-or ? expression : assignment-expression`); the middle one
may still hold a comma. Proved by
`filetests/operators/ternary-in-arguments.glsl`.
