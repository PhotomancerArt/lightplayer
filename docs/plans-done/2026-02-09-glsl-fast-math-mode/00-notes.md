# GLSL Fast Math Mode - Planning Notes

## Scope of Work

Add a "fast math" mode for the GLSL q32 (fixed-point) compiler. In fast math mode, arithmetic operations that can be performed in 32-bit (add, sub) emit inline instructions (`iadd`, `isub`) instead of saturating builtin calls. This trades overflow safety for performance.

- **Operations in scope (fast path)**: add, sub (32-bit inline: `iadd`, `isub`)
- **Operations out of scope (remain builtins)**: mul, div, fma (require i64 for correct semantics; Cranelift fork has limited i64 support)
- **Already inline**: fneg (uses `ineg`), fabs (conditional select)

## Current State of Codebase

### Q32 arithmetic conversion

- **Add/Sub**: `convert_fadd` and `convert_fsub` in `lp-glsl/lp-glsl-compiler/src/backend/transform/q32/converters/arithmetic.rs` always emit a call to `__lp_q32_add` or `__lp_q32_sub` builtins
- **Mul/Div**: Same pattern with `__lp_q32_mul` and `__lp_q32_div` - these use i64 internally and must stay as builtins
- **Neg/Abs**: Already inline - `convert_fneg` uses `builder.ins().ineg()`, `convert_fabs` uses `icmp` + `select`

### Builtin implementations

- `__lp_q32_add` / `__lp_q32_sub`: Rust functions that widen to i64, compute, saturate to [MIN_FIXED, MAX_FIXED], return i32
- Each call incurs: function prologue/epilogue, argument passing, call/return overhead

### Q32 transform configuration

- `Q32Transform` in `lp-glsl/lp-glsl-compiler/src/backend/transform/q32/transform.rs` takes only `FixedPointFormat` (Fixed16x16 or Q32x32)
- No compile-time options exist for math mode
- Transform is used from `lp-glsl-compiler/src/frontend/mod.rs` when compiling for q32

### Call flow for q32

- `Q32Transform::transform_function` -> `convert_all_instructions` -> `convert_fadd` / `convert_fsub`
- `func_id_map` is required by add/sub for builtin lookup; would not be needed for inline path

## Questions

### Q1: How should fast math mode be selected?

**Context**: The mode must be configurable so users can choose between saturating (correct, slower) and wrapping (fast) behavior.

**Suggested options**:
- **Option A**: Add to `Q32Transform`, e.g. `Q32Transform::new(format).with_fast_math(true)` - compiler-level
- **Option B**: Shader pragma / directive, e.g. `#pragma lp_fast_math` - per-shader
- **Option C**: Compiler config struct that gets passed through the pipeline - most flexible

**Answered**: Add `fast_math: bool` to `GlslOptions` (default false), pass through to `Q32Transform` when constructing. Matches existing options flow.

### Q2: Semantics of fast math add/sub: wrapping vs other behavior?

**Context**: In fast math mode, we replace saturating builtins with plain `iadd`/`isub`. Cranelift's `iadd`/`isub` wrap on overflow (2's complement).

**Answered**: Use wrapping semantics (iadd/isub). Document that fast math trades overflow correctness for speed.

### Q3: Should fma be eligible for any optimization?

**Context**: FMA (fused multiply-add) uses `__lp_q32_fma`, which does `a*b + c` with i64 intermediate. It's called from the transform when `fma` opcode is seen.

**Answered**: No change for fma in this plan. Out of scope.

### Q4: How does the mode reach the arithmetic converters?

**Context**: `convert_fadd` and `convert_fsub` currently take `format: FixedPointFormat` and `func_id_map`. They don't receive any "fast math" flag.

**Answered**: Add `fast_math: bool` to `Q32Transform`, pass it through `convert_all_instructions` to `convert_fadd`/`convert_fsub`. Add as a single parameter rather than a new options struct.

### Q5: Default: fast math on or off?

**Context**: Changing default could affect existing shaders that rely on saturation.

**Answered**: Default OFF. Any shader node can opt in. esp32 demo project should use it as an example. Need per-shader-node config (e.g. `glsl_opts`) that can hold fast_math and future GLSL compile options.

### Q6: Where is Q32Transform constructed, and how do we pass the option from the application?

**Context**: Q32Transform is built in `frontend/mod.rs` (compile_glsl_to_gl_module_jit and compile_glsl_to_gl_module_object), `esp32-glsl-jit`, `lp-glsl-q32-metrics-app`, and test utilities. Options flow from `GlslOptions` (run_mode, decimal_format) into the compile functions.

**Answered**: Add `fast_math: bool` to `GlslOptions`. Introduce per-shader-node config (e.g. `glsl_opts`) that flows to compilation and populates GlslOptions.fast_math. esp32 demo project configures its shader node with fast_math enabled.
