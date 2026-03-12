# Migration Roadmap

The migration is incremental. At each phase, the compiler is fully functional.
The existing Q32 transform coexists with direct emission until validation is
complete, at which point it can be removed.

## Plan A: NumericStrategy trait + FloatStrategy (no behavioral change)

### Goal
Introduce the abstraction without changing any output. The compiler produces
identical IR to today. This is the structural refactor.

### Work
1. Define the `NumericStrategy` trait (or enum) with all methods.
2. Implement `FloatStrategy` — each method delegates to the corresponding
   CLIF instruction (fadd, fmul, f32const, etc.). This is trivial.
3. Add `numeric: NumericMode` to `CodegenContext`.
4. Update all ~25 inline operation call sites in the codegen to route through
   the strategy instead of calling `builder.ins()` directly.
5. Update `SignatureBuilder` to accept the numeric scalar type.
6. Run the full test suite. Output must be bit-identical.

### Risk
Low. Each call site change is mechanical. The FloatStrategy produces exactly
the same instructions as the hardcoded calls.

### Estimated scope
~50 lines of new code (trait + FloatStrategy), ~25 call site edits.

---

## Plan B: Q32Strategy for inline operations

### Goal
Implement the Q32 numeric strategy for arithmetic, comparisons, constants,
conversions, and rounding. This is the core math logic, extracted from the
existing Q32 transform.

### Work
1. Implement `Q32Strategy` with `Q32Options` for configurable behavior
   (saturating vs wrapping add/sub, multiply modes, etc.).
2. Each method's logic is extracted from the existing transform code:
   - `emit_add` ← `convert_fadd` in `instructions.rs`
   - `emit_mul` ← `convert_fmul` in `instructions.rs`
   - `emit_const` ← `convert_f32const` in `instructions.rs`
   - `emit_cmp` ← `convert_fcmp` in `instructions.rs`
   - etc.
3. Unit-test each method independently: given known Q32 inputs, verify the
   emitted instruction sequence is correct.
4. Cross-validate against the transform: compile a function with both paths,
   compare the CLIF IR output instruction by instruction.

### Risk
Medium. The Q32 math logic is well-understood (it exists in the transform),
but re-expressing it in the emission context may surface edge cases. The
cross-validation against the transform mitigates this.

### Estimated scope
~200 lines of Q32Strategy implementation, ~100 lines of tests.

---

## Plan C: Numeric-aware builtin dispatch

### Goal
Make the builtin function call paths (math libcalls, LPFX functions, inline
expansions) select the right implementation based on numeric mode.

### Work
1. Refactor `get_math_libcall` / `get_math_libcall_2arg` in
   `builtins/helpers.rs` to accept numeric mode. For Q32, look up the
   corresponding BuiltinId instead of creating a TestCase name.
2. Extract the `map_testcase_to_builtin` mapping from
   `backend/transform/q32/calls.rs` into a shared location.
3. Update `builtins/trigonometric.rs` (sin, cos, tan, etc.) and
   `builtins/common.rs` (pow, exp, log, etc.) to use the new helpers.
4. Update `lpfx_fns.rs` to select float vs Q32 variant directly.
5. Add Q32 inline expansions for fract, sign, isinf, isnan to the
   corresponding builtin emission functions.
6. Test: compile shaders with Q32 direct emission, verify correct builtin
   calls in the output IR.

### Risk
Medium. The builtin ecosystem has many functions. The LPFX dispatch in
particular has complex argument handling (vector flattening, out/inout
parameters, struct returns). However, the call mechanics don't change —
only which function is called.

### Estimated scope
~150 lines of dispatch changes, ~50 lines of inline expansion additions.

---

## Plan D: Wire up the pipeline

### Goal
Connect the direct Q32 emission to the compilation pipeline. A shader
compiled with `DecimalFormat::Q32` uses the Q32Strategy directly, bypassing
the transform entirely.

### Work
1. In `glsl_jit` / `glsl_jit_streaming`: when `decimal_format == Q32`,
   pass `Q32Strategy` as the numeric mode to the codegen.
2. Build signatures using `strategy.map_signature()` instead of calling
   `transform.transform_signature()` separately.
3. Declare functions with Q32 signatures from the start — no float module
   needed.
4. For the streaming path: single module, single pass. The entire
   `float_module` + `transform_single_function` + `func_id_map` machinery
   becomes unnecessary.
5. For the batch path: single `compile_glsl_to_gl_module_jit` call
   produces Q32 IR directly, no `apply_transform` step.
6. Full integration test: compile the test shader suite with direct Q32,
   compare output values against the transform-based path.

### Risk
Low-medium. The individual components are validated in plans B and C. This
is plumbing. The main risk is missing a codegen path that still emits float
operations without going through the strategy.

### Estimated scope
~100 lines of pipeline changes, removal of ~50 lines of transform plumbing.

---

## Plan E: Validation and cleanup

### Goal
Verify correctness, measure memory improvement, remove the transform path.

### Work
1. Run the full shader test suite with both paths. Compare output values
   at sufficient precision (Q32 has inherent precision differences vs float,
   but the two Q32 paths should be identical).
2. Run ESP32 heap traces. Verify memory improvement.
3. Benchmark compilation time (direct emission should be faster).
4. Once validated, remove:
   - `backend/transform/q32/` (the entire transform)
   - `apply_transform` from `GlModule`
   - `TransformContext`, `transform_single_function`
   - The `Transform` trait (unless the identity transform is still used)
   - The float module creation in the streaming path
   - `func_id_map` / `old_func_id_map` machinery
5. Simplify the JIT build paths: with no transform, `build_jit_executable`,
   `build_jit_executable_memory_optimized`, and
   `build_jit_executable_streaming` can likely be consolidated.
6. Update documentation and configuration.

### Risk
Low. This is cleanup after validation. The transform code is retained in
git history.

### Decision point
If direct emission doesn't match the transform output, investigate and fix
before removing the transform. The transform can remain as a fallback or
test reference indefinitely.

---

## Ordering and dependencies

```
Plan A (trait + FloatStrategy)
  → Plan B (Q32Strategy inline ops)
  → Plan C (builtin dispatch)      [B and C are somewhat independent]
    → Plan D (wire up pipeline)
      → Plan E (validate + cleanup)
```

Plans B and C can be worked in parallel or in either order. Plan D requires
both. Plan E requires D.

Each plan is self-contained and can be a separate PR/commit series.

## What this enables for streaming

After plan D, the streaming pipeline becomes:

```
Parse → TypedShader
For each function (sorted by size):
  → codegen with Q32Strategy → Q32 CLIF IR (one module, one pass)
  → define_function → machine code
  → drop CLIF IR
Finalize
```

One module. No transform. No float module. No func_id_map. The streaming
overhead drops to just the per-function bookkeeping, and the savings from
only having one function's IR in memory at a time are no longer offset by
the two-module tax.

## Keeping the transform (optional)

The Q32 transform doesn't have to be deleted. It could remain as:

- A test oracle (compile with transform, compare against direct emission)
- A fallback for debugging
- A reference implementation for new numeric strategies

If retained, it should be gated behind a feature flag and excluded from
the default build to avoid code size overhead on the ESP32.
