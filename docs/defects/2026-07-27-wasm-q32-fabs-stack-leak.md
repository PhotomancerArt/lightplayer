---
status: fixed
found: 2026-07-27      # how: live shader-agent sessions (agent-written shaders as fuzzer)
fixed: this change
area: lpvm-wasm emit (+ lpvm-cranelift trunc semantics found by the new coverage)
class: inline-emit stack imbalance masked by unreachable block ends
---
# Q32 `abs()` leaked a wasm stack value; blamed as "break/continue in nested loops"

**Symptom** — Two live shader-agent sessions (2026-07-27, branch
`claude/agent-params-polish`) had GLSL that compiled through naga's
glsl-in frontend but was rejected by the wasm runtime backend with

```
shader WASM parse/validate failed: ... expected 0 elements on the stack for fallthru, found 2
```

The first session correlated the failure with `break`/`continue` inside
nested loops, worked around it with if-guards, and the observation was
codified as a prompt "dialect landmine" (commit `5be0bb876` on the agent
branch). The second session bisected 14 steps to a different trigger:
`abs()` inside a loop's if-branch that also stores to an array —
including the detail that `sqrt(x*x)` compiled where `abs(x)` didn't.

**Root cause** — `emit_q32_fabs` in `lp-shader/lpvm-wasm/src/emit/q32.rs`
pushed `src` twice but consumed it once: a dead `local.get(src)` before
the `if (result i32)` sequence leaked exactly one value per `abs()`
call. The leak was invisible almost everywhere because a function body's
trailing `return` makes the function's final `end` unreachable, where
wasm skips the stack-balance check — every existing filetest used `abs`
at function top level. Inside any `if`/`loop` block whose `end` is
reachable, validation fails. Loops made the failure *look* like a
control-flow bug: the leaked value crosses the loop body's block end.
Break/continue in nested loops was never broken — 30+ shape probes plus
the new `brknest_*` torture axis (3,785 directives) pass on all five
targets both before and after the fix.

**Fix** — Delete the dead push (one line). The wasm backend now matches
lpvm-native's branchless fabs and the interp semantics; `i32.sub` from 0
wraps at `i32::MIN` exactly like `wrapping_neg` on rv32.

**Fallout found by the new coverage** — `lpvm-cranelift`'s
`emit_ftrunc` masked fraction bits only, i.e. floor semantics — wrong
for negatives (`trunc(-0.75)` → `-1.0`, should be `0.0`). rv32c now
adjusts negative-with-fraction results by +1.0, matching wasm/native/
interp. Caught by `control/edge_cases/ops-in-nested-blocks.glsl` because
its operands derive from loop variables instead of constants.

**Regression coverage**
- `builtins/common-abs.glsl` — `abs()` had no dedicated filetest at all.
- `control/edge_cases/ops-in-nested-blocks.glsl` — every Q32 op family
  that inline-expands to wasm `if`/`else`/`end` (abs, min/max,
  floor/ceil/trunc, int/uint casts, divide) evaluated inside an
  if-inside-loop, where a reintroduced leak fails compilation on every
  backend; plus the live-session shape verbatim (abs in the inner loop
  of a nested pair with continue, break, and an array store).
- `control/torture/brknest_*` (10 new generated files) — nested loop
  pairs in all outer-kind x inner-kind combinations with inner
  break+continue, outer break/continue after the inner loop, and
  depth-3 chains, so the next misattribution of this class can be ruled
  out in one corpus run.

**Lesson** — When a backend rejects control flow that the frontend
accepted, suspect a stack-balance leak in an *expression* op before
believing the control flow is at fault: wasm only checks stack balance
at reachable block ends, so the leak surfaces exactly where interesting
control flow lives and pattern-matches to "loops are broken". The
`sqrt(x*x)`-works-but-`abs(x)`-doesn't observation was the tell —
`sqrt` lowers to a balanced import call, `abs` to inline emission.
