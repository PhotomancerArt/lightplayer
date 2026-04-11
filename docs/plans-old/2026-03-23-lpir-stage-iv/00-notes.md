# Stage IV: Naga → LPIR Lowering — Planning Notes

## Scope

Implement `lps-frontend/src/lower.rs` — the lowering pass that converts a
`naga::Module` into an `IrModule` of LPIR functions. Covers scalar expressions,
control flow, user function calls, math builtin decomposition, and LPFX call
structure. The lowering is completely float-mode-unaware.

**In scope:**

- Expression lowering: literals, arguments, locals (load/store), binary ops,
  unary ops, comparisons, casts, select, zero values, constants
- Statement lowering: emit (no-op), block, if/else, loop, break, continue,
  return, store, call
- Expression caching: `Vec<Option<VReg>>` indexed by `Handle<Expression>`
- Parameter aliasing: detect `Store(LocalVariable, FunctionArgument)` pattern,
  alias the local's VReg to the parameter's VReg
- User function calls: `Statement::Call` → `Op::Call`
- LPFX calls: detect LPFX builtins, generate `@lpfx::...` imports with
  slot-based out-parameter ABI
- Math builtin decomposition/import: abs, round, min, max, mix, smoothstep,
  step, mod
- Tests: GLSL → Naga → LPIR → interpret, verify results
- Tests: GLSL → Naga → LPIR → print text, verify output

**Out of scope:**

- Vector expressions (future follow-on)
- Vector builtins (future follow-on)
- WASM emission from LPIR (Stage V)

## Current state

### `lps-frontend` crate (`lp-shader/lps-frontend/`)

Thin wrapper around `naga::front::glsl`. Provides:

- `compile(source) → NagaModule` (parse GLSL, collect function metadata)
- `NagaModule { module: naga::Module, functions: Vec<(Handle<Function>, FunctionInfo)> }`
- `FunctionInfo { name, params: Vec<(String, GlslType)>, return_type: GlslType }`
- Prepends LPFX prototypes and a dummy `main` entry point
- `#![no_std]`, depends only on `naga`

### `lps-wasm` crate (existing Naga → WASM emitter)

The reference for what Naga IR patterns the lowering must handle. Key files:

- `emit.rs` (~1970 lines): walks Naga statements and expressions, emits WASM
  instructions directly. Handles scalars + vectors, mode-aware (float/Q32).
- `locals.rs` (~310 lines): WASM local allocation, parameter aliasing detection,
  CallResult tracking, scratch pool.
- `lpfx.rs` (~330 lines): LPFX builtin resolution, out-pointer ABI via scratch
  memory, call emission.

The WASM emitter covers these expression types (scalar path):

- `Literal`, `Constant`, `FunctionArgument`, `CallResult`, `Load(LocalVariable)`
- `Binary` (all Naga binary ops including LogicalAnd/Or, bitwise, shifts)
- `Unary` (Negate, LogicalNot, BitwiseNot)
- `Select`, `As` (casts), `ZeroValue`
- `Math` (Mix, SmoothStep, Step, Round, Abs, Min, Max)

Statement types:

- `Emit` (no-op for pure expressions, emit+drop for side-effectful)
- `Block`, `If`, `Loop` (with do-while trailing guard splitting), `Break`,
  `Continue`, `Return`, `Store`, `Call`

### `lpir` crate

Complete as of Stage III. Key API surface for the lowering:

- `FunctionBuilder` / `ModuleBuilder` for construction
- `Op` enum with all arithmetic, comparison, control flow, memory, call ops
- `IrType`: `F32` | `I32` only (booleans and uints use `I32`)
- `ImportDecl { module_name, func_name, param_types, return_types }`
- `interpret()` + `ImportHandler` trait for testing
- `validate_module()` / `validate_function()` for verification
- `print_module()` / `parse_module()` for text round-trips

### Naga expression/statement model (relevant patterns)

- Naga expressions form a DAG (arena-indexed). A single expression may be
  referenced by multiple statements/other expressions.
- `Statement::Emit(range)` marks expressions for evaluation. The WASM emitter
  treats pure expressions as no-ops.
- Naga `in` parameters: modeled as `LocalVariable + Store(FunctionArgument)`.
  Must be detected and aliased to avoid unnecessary copies.
- `Loop { body, continuing, break_if }`: `continuing` holds increment code for
  `for` loops; `break_if` holds loop exit condition. `Continue` should target
  the continuing block, not the loop head.
- LPFX functions: parsed as bodyless functions in `naga::Module` via the
  prologue; resolved by name pattern `lpfx_*` + parameter type overloading.

## Questions

### Q1: Crate placement and dependency

The roadmap places `lower.rs` in `lps-frontend`, requiring `lpir` as a new
dependency of `lps-frontend`.

**Current state:** `lps-frontend` depends only on `naga`. Adding `lpir` creates
a dependency: `lps-frontend` → `lpir` (and `lps-frontend` → `naga`). Both are
`no_std` + `alloc`, so compatible.

**Alternative:** Create a new crate (e.g. `lps-lower`) that depends on both
`lps-frontend` and `lpir`.

**Suggested:** Follow the roadmap. Add `lpir` dependency to `lps-frontend`. The
lowering is tightly coupled to the Naga frontend's output (`NagaModule`) and
keeping them together avoids crate proliferation.

**Answer:** Follow the roadmap. `lower.rs` in `lps-frontend`, add `lpir` dep.

### Q2: Math builtin handling

The roadmap says "Math builtins... decomposed into scalar LPIR ops." But some
builtins need library functions that LPIR doesn't have as core ops (e.g. `round`
needs `floor` or a native round, `mod` needs `trunc`/`floor`).

The `std.math` import module provides `fround`, `fmod`, `fabs`, `fmin`, `fmax`,
`fmix`, `fsmoothstep`, `fstep`, etc. Two strategies:

**Option A — Maximize inline decomposition:**

- Decompose into core LPIR ops: abs, min, max, step, mix, smoothstep
  (all expressible via fadd/fsub/fmul/fdiv + comparisons + select + constants)
- Emit as `@std.math::...` imports: round, mod (need floor/trunc which aren't
  core LPIR ops)

**Option B — All math builtins as imports:**

- Every `MathFunction` maps to `call @std.math::fname(...)`. Simpler lowering,
  but the interpreter needs a comprehensive StdMathHandler and emitters need to
  handle more import calls.

**Option C — Hybrid (suggested):**

- Decompose: mix, smoothstep, step (these are straightforward arithmetic
  sequences in mode-agnostic LPIR)
- Import: abs, round, min, max, mod (clean single-call semantics; emitters
  already know how to implement these per-mode)

**Suggested:** Option C — decompose the multi-op builtins (mix, smoothstep,
step) since they're just arithmetic; use `@std.math::...` imports for builtins
that map naturally to single target instructions (abs, round, min, max, mod).

**Answer:** Add 8 new LPIR primitive ops for things both Cranelift and WASM have
as native single instructions: `Fabs`, `Fsqrt`, `Fmin`, `Fmax`, `Ffloor`,
`Fceil`, `Ftrunc`, `Fnearest` (roundEven semantics). Then three tiers:

1. **Direct LPIR op**: abs, sqrt, min, max, floor, ceil, trunc, roundEven
2. **Inline decomposition**: mix, smoothstep, step, mod, fract, clamp, sign
   (composed from core LPIR ops + the new primitives)
3. **Import calls** (`@std.math::...`): round (ties-away-from-zero), sin, cos,
   tan, asin, acos, atan, atan2, sinh, cosh, tanh, asinh, acosh, atanh, exp,
   log, exp2, log2, pow, inversesqrt, fma, ldexp

Integer abs/min/max decompose into existing LPIR ops (compare + select/negate).
Emitters handle Q32 mode for the new ops (e.g. `Fsqrt` → `__lp_q32_sqrt` call,
`Ffloor` → shift/mask).

### Q3: Naga `Loop` with `continuing` / `break_if`

LPIR's `Continue` always jumps to `LoopStart`. But Naga's `Loop` has a
`continuing` block (for `for` loop increments) that `Continue` should execute
before looping. This mismatch needs a lowering strategy.

**Current WASM approach:** Triple-block nesting (`block { loop { block { body }
continuing; break_if; br 0; } }`). WASM has relative branch depths, so
`Continue` can target the inner block boundary.

**Proposed LPIR approach — first-iteration flag:**

```
v_first:i32 = iconst.i32 1
loop {
    if v_first {
        // skip continuing on first iteration
    } else {
        // continuing block
        // break_if → break
    }
    v_first = iconst.i32 0
    // body (Continue → loop head → flag=0, executes continuing)
}
```

When `continuing` is empty and `break_if` is None (while/do-while), emit a
simple loop with no flag overhead.

**Suggested:** Use the first-iteration flag pattern only when `continuing` is
non-empty or `break_if` is present. Simple loops for the common case.

**Answer:** Spec gap fixed in `docs/plans-done/2026-03-23-lpir-continuing.md`.
`LoopStart` now has `continuing_offset`. `Continue` jumps to
`continuing_offset` (defaults to body start when no continuing section).
Builder has `push_continuing()`. Text format uses `continuing:` label.
Validator enforces `Continue` not inside the continuing section of the
enclosing loop. No workaround needed — the lowering emits `push_continuing()`
for Naga's `continuing` block directly. Implemented via `continuing_offset` on `LoopStart`. See
`docs/plans-done/2026-03-23-lpir-continuing.md` for details. Continue jumps
to `continuing_offset`; End loops back to body start without popping. Two
latent interpreter bugs fixed (End popping loop frame, Continue not cleaning
up intervening If frames).

### Q4: LPFX scope in Stage IV

The roadmap includes LPFX: "detect LPFX builtins, generate memory ops for
out-params." In LPIR, this means:

1. Detect `lpfx_*` calls in Naga IR
2. Create `@lpfx::builtin_name(...)` import declarations in the IrModule
3. For out-parameters: allocate slots via `FunctionBuilder::alloc_slot`,
   pass slot address as i32 arg, load results from slot after call

The existing LPFX resolution logic lives in `lps-wasm/src/lpfx.rs` and
depends on `lps-builtin-ids` for `BuiltinId` resolution.

**Suggested:** Implement full LPFX lowering as described. This requires adding
`lps-builtin-ids` as a dependency of `lps-frontend` (for `BuiltinId`
resolution) or reimplementing the name-based mapping.

**Alternative:** Defer LPFX to a follow-up. Stage IV covers only user functions
and `std.math` builtins. LPFX testing without a runtime executor is limited to
print-output verification anyway.

**Answer:** A — include LPFX in Stage IV. Add `lps-builtin-ids` dependency
to `lps-frontend`. Test via print-output verification.

### Q5: `uint` and `bool` type mapping

LPIR has only `F32` and `I32`. Naga distinguishes `Float`, `Sint`, `Uint`,
`Bool` scalar kinds.

**Suggested mapping:**

- `Float` → `IrType::F32`
- `Sint` → `IrType::I32`
- `Uint` → `IrType::I32` (signedness per-operation: `IdivU`, `IltU`, etc.)
- `Bool` → `IrType::I32` (0 = false, non-zero = true)

This matches the WASM emitter's approach (both `int` and `uint` map to `i32`;
`bool` is `i32` 0/1).

**Suggested:** Follow this mapping. No changes to LPIR types needed.

**Answer:** Yes. Float→F32, Sint/Uint/Bool→I32. Signedness per-operation.

### Q6: Testing strategy

The roadmap specifies two test types:

1. GLSL → Naga → LPIR → interpret → verify results
2. GLSL → Naga → LPIR → print text → verify output

For (1), the interpreter needs an `ImportHandler` to handle `@std.math::...`
calls (if we use imports for some math builtins per Q2).

**Suggested:**

- Build a `StdMathHandler` implementing `ImportHandler` that handles `std.math`
  module calls using Rust's `f32` operations (sin, cos, round, etc.)
- Test helpers: `lower_and_run(glsl, func, args) → Vec<Value>` and
  `lower_and_print(glsl) → String`
- For LPFX: print-output verification only (no interpreter execution)
- Also run `validate_module()` on every lowered module to catch structural bugs

**Answer:** Tests in separate files. `StdMathHandler` in its own module
(reusable by both lower tests and future consumers). Test files under
`lps-frontend/tests/` for end-to-end GLSL→LPIR tests. Validate every
lowered module. LPFX: print-output only.
