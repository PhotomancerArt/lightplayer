# Stage V Review 1 — Remaining Work

Audit date: 2026-03-24

## Summary

Phases 01–07 are functionally complete. The LPIR → WASM pipeline
compiles, emits valid WASM for scalar shaders, and passes 14 smoke
tests covering integer ops, Q32 arithmetic, control flow, and memory.
Remaining work is concentrated in two areas: correctness gaps in Q32
cast emission (Phase 03), and missing test coverage / cleanup items
(Phase 08).

## Functional gaps

### Q32 `ItofS` / `ItofU` — missing i64 saturation

**Plan (03-q32-expansion.md):** widen to i64, shl 16, saturate via
`emit_q32_sat_from_i64`.

**Current:** `i32_shl(16)` only — no widening. For input values
outside roughly [-32768, 32767], the shift overflows silently.
Example: `ItofS` on `32768` produces `0` instead of `Q32_MAX`.

**Fix:** extend to i64, shift, saturate, wrap — same pattern as
`Fmul`. Needs the i64 scratch local (add `Op::ItofS`/`ItofU` to the
`func_needs_i64_scratch` check).

### `Fsqrt` / `Fnearest` — import dependency

**Plan (03-q32-expansion.md, 06-calls-and-imports.md):** these ops
always resolve to builtins; the emitter should be self-sufficient.

**Current:** `ops.rs` calls `imports::std_math_callee(ir, "sqrt")`
which searches `ir.imports` for a matching `@std.math` entry. This
works for the Naga→LPIR pipeline (lowering registers `sqrt`/`round`
since last session), but fails for hand-written LPIR that contains
`Fsqrt` / `Fnearest` without a corresponding import.

**Acceptable for now:** the only producer of `IrModule` is
`lps-naga::lower`, which always registers these imports. Document
this coupling; revisit if a second IR producer appears.

### `$sp` global index — hard-coded `0u32`

**Current:** `emit/mod.rs` line 101 uses `Some(0u32)`.

**Risk:** correct as long as `$sp` is the only global. If import
globals or additional module globals are added later, this breaks.

**Acceptable for now:** no other globals exist. Add a comment
noting the assumption.

## Switch emission — known limitations

The switch implementation uses chained `if (selector == value)` per
case, not `br_table`. This is explicitly allowed by the plan
(04-control-flow.md §Switch) as the simpler initial approach. Known
edge cases:

- **Non-returning case arms** (fallthrough to default or next case
  without `return` / `break`): the `unwind_ctrl_after_return` logic
  handles the return-from-case path, but a case that falls through
  to code after the switch block is not exercised.
- **Empty cases / fall-through chains:** not tested.

These are unlikely to appear in Naga-lowered GLSL (Naga always emits
`break` or `return` per case), so this is low priority.

## Phase 08 — Tests + cleanup

### Tests present (14)

| Test | Coverage |
|------|----------|
| `float_literal_return` | Float mode literal |
| `float_add` | Float mode add |
| `int_add_typed` | i32 add (Q32 default) |
| `multiple_functions_exported` | Float mode multi-export |
| `q32_add` | Q32 fadd with saturation |
| `q32_mul` | Q32 fmul (i64 path) |
| `q32_div` | Q32 fdiv |
| `q32_abs` | Q32 fabs (inline branch) |
| `q32_while_accumulates` | Loop + accumulator |
| `int_switch_dispatch` | Switch with 3 cases |
| `q32_floor_and_ceil` | Q32 ffloor / fceil |
| `q32_chained_float_compare_and` | If + logical and |
| `q32_chained_float_compare_or` | If + logical or |
| `q32_triple_float_compare_and` | Nested logical chain |

### Tests missing (from Phase 08 plan)

**Arithmetic:**
- `q32_subtract` — float subtraction
- `q32_negate` — float negation
- `q32_int_modulo` — integer `%`

**Control flow:**
- `q32_if_else` — conditional return (simple)
- `q32_for_loop` — for-style loop
- `q32_nested_loops` — nested loop with break

**Math:**
- `q32_min_max` — `min(a, b)`, `max(a, b)`
- `q32_mix` — `mix(a, b, t)` (inline decomposition)
- `q32_clamp` — `clamp(x, lo, hi)`
- `q32_step` — `step(edge, x)`

**Calls:**
- `q32_call_user_func` — call between two exported funcs
- `q32_call_chain` — A→B→C call chain

**Casts:**
- `q32_float_to_int` — `FtoiSatS`
- `q32_int_to_float` — `ItofS`

### Test helpers

- `run_q32_i32` helper: not yet added.
- `run_f32` / Float-mode tests: still present, not updated to Q32
  or `#[ignore]` per plan.

### Cleanup items

- `smoke.rs` line 1: header comment still says "Naga-based pipeline"
  — should say "LPIR-based pipeline" or just "wasmtime smoke tests".
- `$sp` global index `0u32`: add comment documenting assumption.
- No `README.md` audit done (old layout references may remain).
- `cargo clippy -D warnings` and `cargo +nightly fmt --check` pass
  as of this review.

## Priority ordering

1. **Fix Q32 `ItofS`/`ItofU`** — correctness bug, small change.
2. **Add missing smoke tests** — fill coverage gaps before Stage VI
   integration testing.
3. **Update Float-mode tests** — `#[ignore]` or convert to Q32.
4. **Add `run_q32_i32` helper** — needed by several missing tests.
5. **Minor cleanup** — comments, header, `$sp` assumption note.

Items 1–3 are blockers for calling Stage V complete. Items 4–5 are
nice-to-have before Stage VI.
