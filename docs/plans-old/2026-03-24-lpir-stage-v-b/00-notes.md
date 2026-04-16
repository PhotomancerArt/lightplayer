# Stage V-B: Filetest Failure Fixes — Notes

## Scope of work

Fix the ~40 failing GLSL filetests that surfaced after Stage IV-B / Stage V
work. These span the Naga→LPIR lowering (`lps-frontend`), the WASM emitter
(`lps-wasm`), LPFX import resolution, and test maintenance.

## Current state

27 WASM smoke tests pass. The LPIR→WASM pipeline (Stage V) is functionally
complete for scalar shaders. However, running the full filetest suite reveals
failures in several categories.

## Failure categories

### Bug 1: `continue` in nested loops targets wrong loop

**Files:** `control/while/continue.glsl`, `control/for/continue_nested.glsl`,
`control/while/nested_for.glsl`

**Symptom:** `continue` inside an inner loop skips nothing or targets the outer
loop. `test_continue_nested_triple()` expected 18, got 24 (continue completely
ignored). `test_for_in_while_loop_continue()` expected 12, got 15.

**Root cause:** Likely in `lower_stmt.rs` — the `Continue` statement lowering
may emit a `continue` that targets the wrong loop nesting level when multiple
loops are nested.

**Priority:** P0 — correctness bug affecting real shaders.

### Bug 2: `bool()` constructor / As with non-32-bit types

**Files:** `scalar/bool/from-bool.glsl`, `scalar/bool/from-float.glsl`,
`scalar/bool/from-int.glsl`, `scalar/bool/from-uint.glsl`

**Symptom:** `error[E0400]: unsupported expression: As with non-32-bit byte
convert`

**Root cause:** `expr_scalar.rs` doesn't handle Naga `As` expressions where
the target type is `Bool` (1-byte). Only 4-byte (i32/u32/f32) casts are
handled.

**Priority:** P0 — 4 test files, affects any shader using `bool(x)`.

### Bug 3: Implicit type conversion in ternary results

**Files:** `control/ternary/type_conversions.glsl`

**Symptom:** `test_ternary_float_to_int_conversion()` expected 10, got 675020.
The value 675020 ≈ 10.3 × 65536, meaning the Q32 representation is returned
raw as an "int" without the float→int conversion being applied.

**Root cause:** When a ternary has float branches but the result is assigned to
an int, the implicit `As` (float→int) wrapping the ternary result isn't
lowered correctly — the Q32 raw bits leak through.

**Priority:** P0 — correctness bug, but only 1 test case affected in this
file. May also affect other implicit conversion paths.

### Bug 4: Function prototypes (forward declarations) not resolved

**Files:** `function/declare-prototype.glsl`

**Symptom:** `error[E0101]: undefined function 'add_two_floats'` — functions
defined after their call site aren't found.

**Root cause:** The Naga→LPIR lowering processes functions in source order and
doesn't do a pre-pass to register all function signatures before lowering
bodies.

**Priority:** P0 — affects any shader using forward declarations. Common in
real GLSL code.

### Bug 5: `inout` parameters (Pointer types)

**Files:** `function/edge-inout-both.glsl`

**Symptom:** `error[E0400]: unsupported type: Pointer { base: [3], space:
Function }`

**Root cause:** Naga represents `inout` parameters as pointer types. The LPIR
lowering doesn't handle `TypeInner::Pointer`.

**Priority:** P0 — affects any shader using `inout`/`out` parameters. Very
common in GLSL.

### Bug 6: Q32 `round(2.5)` returns 2.0 instead of 3.0

**Files:** `const/builtin/extended.glsl`

**Symptom:** `test_builtin_trunc_round_ceil_mod()` expected 9.0, got 8.0.
Passes on `cranelift.q32`, fails on `wasm.q32`. The difference is `round(2.5)`
= 2.0 (wasm) vs 3.0 (cranelift).

**Root cause:** The WASM emitter routes `Fnearest` through `@std.math::round`
which calls `libm::roundf` in the builtins module. `libm::roundf(2.5)` = 3.0
(round half away from zero), but the const evaluator or wasm builtins
implementation may use banker's rounding.

**Priority:** P1 — only affects exact 0.5 midpoints. 1 test case.

### Missing feature 7: Array type declarations

**Files:** `array/declare-explicit.glsl`

**Symptom:** `error[E0400]: unsupported type for LPIR: Array { ... }` for
`bvec4 arr[3]`.

**Root cause:** LPIR doesn't have array types yet. Most tests in this file
pass because they only declare arrays then return scalars — only the bvec4
array test hits the unsupported type path.

**Priority:** P2 — 1 test case. Arrays are a Stage VI topic.

### Missing feature 8: LPFX import overloads

**Files:** `lpfx/lp_saturate.glsl`, `lpfx/lp_gnoise.glsl`,
`lpfx/lp_fbm.glsl`, `lpfx/lp_hash.glsl`, `lpfx/lp_hsv2rgb.glsl`,
`lpfx/lp_hue2rgb.glsl`, `lpfx/lp_psrdnoise.glsl`, `lpfx/lp_random.glsl`,
`lpfx/lp_rgb2hsv.glsl`, `lpfx/lp_simplex2.glsl`, `lpfx/lp_simplex3.glsl`,
`lpfx/lp_srandom.glsl`

**Symptom:** `unsupported lpfx import 'lpfx_saturate_4' with [Float, Float,
Float]` — certain LPFX function overloads with specific parameter type
combinations aren't registered.

**Root cause:** `lower_lpfx.rs` doesn't register all overload variants for
LPFX functions. The vec4 and mixed-type overloads are missing.

**Priority:** P1 — 12 test files, but this is LPFX-specific plumbing, not
a logic bug.

### Stale markers 9: Unexpected passes (`@unimplemented` now passing)

**Files:** `control/for/complex-condition.glsl`,
`operators/predec-scalar-float.glsl`, `operators/predec-scalar-int.glsl`,
`operators/predec-vec2.glsl`, `operators/preinc-mat3.glsl`,
`operators/preinc-vec2.glsl`, `operators/incdec-scalar.glsl`,
`operators/incdec-vector.glsl`, `operators/incdec-matrix.glsl`

**Symptom:** Tests marked `@unimplemented(backend=wasm)` now pass on wasm.q32.

**Fix:** Run `scripts/glsl-filetests.sh --fix` to strip markers.

**Priority:** P2 — no code change needed, just test maintenance.

### Test bugs 10: Commented-out functions referenced by run directives

**Files:** `function/overload-ambiguous.glsl`,
`function/recursive-static-error.glsl`

**Symptom:** `function 'test_overload_ambiguous_promotions' not found` — the
run directive references a function inside a `/* */` comment block.

**Fix:** Either comment out the run directive or wrap the entire block
(function + directive) consistently.

**Priority:** P2 — test file bugs, not code bugs.

### Test expectation 11: Rainbow blessed values are placeholders

**Files:** `debug/rainbow.glsl`

**Symptom:** Expected `vec4(0.0, 0.0, 0.0, 0.0)` but got
`vec4(0.268, 0.605, 0.265, 1.0)`. The expected values are all-zero
placeholders.

**Fix:** Re-bless the expected values from cranelift.q32 output.

**Priority:** P2 — test maintenance.

### Test expectation 12: `float(INT_MAX)` saturation edge case

**Files:** `scalar/float/from-int.glsl`

**Symptom:** `test_float_from_int_large()` expected 32767.0, got 32768.0.
`ItofS(2147483647)` saturates to Q32_MAX = `i32::MAX` = 32767.99998...
which rounds to 32768.0 in f32.

**Fix:** Update test expectation to ~= 32768.0 (or verify saturation is
correct and adjust tolerance).

**Priority:** P2 — 1 test case, edge behavior.

## Questions

### Q1: Scope — which categories to include?

The failures break into three tiers:

- **P0 bugs** (1–5): correctness bugs in the lowering layer. These break real
  shaders.
- **P1 missing features** (6–8): Q32 round midpoint, LPFX overloads. These
  are gaps but not regressions.
- **P2 maintenance** (9–12): stale markers, test expectations, test file bugs.

**Suggested scope:** P0 bugs (1–5) plus the P2 quick-fixes (9–12) since
they're trivial. P1 items (6–8) can be deferred or included if time permits.

### Q2: `inout`/`out` parameter strategy

Naga represents `inout` as Pointer types. Two approaches:

A) **Pure value-based extra returns:** callee returns final values of `inout`
params as additional results. Keeps LPIR pointer-free.
B) **Slot-based (mirrors Cranelift):** caller allocates LPIR slots for `inout`
args, stores current values, passes `SlotAddr` as i32 to callee. Callee
does `Load`/`Store` through the address. Caller loads back after call.

**Answer:** (B) — slot-based, matching Cranelift. Reasons:

- Matches Cranelift ABI (pointer to stack slot for out/inout).
- Handles nested `inout` forwarding naturally.
- Multi-return will use memory (StructReturn) anyway, so the slot-based
  approach avoids introducing a second convention.
- LPIR already has `SlotAddr`, `Load`, `Store` and the WASM emitter handles
  them.

### Q3: Forward declaration resolution strategy

Two approaches:

A) **Two-pass lowering:** first pass collects all function signatures, second
pass lowers bodies. Functions can reference any other function.
B) **Reorder functions:** topologically sort before lowering.

**Answer:** (A) — pre-pass to register all function signatures before lowering
bodies. Naga already has all function metadata upfront.

### Q4: Bool cast approach

Naga `As` with bool target means "compare to zero". In Q32:

- `bool(float_val)` → `float_val != 0` → `i32.ne(v, 0)` (Q32 zero is `0i32`)
- `bool(int_val)` → `int_val != 0`
- `bool(bool_val)` → identity

**Answer:** Add a branch in `expr_scalar.rs` for `As` where target is Bool,
emitting compare-not-equal-to-zero. In Q32, float zero is `0i32`, so all types
use `Ine(src, const_0)`. `bool(bool)` is identity (copy).
