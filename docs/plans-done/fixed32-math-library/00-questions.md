# Questions for Q32 Math Library Plan

## Current State

We have:

- `lp-glsl-builtins` crate with q32 implementations for `div`, `mul`, `sqrt`
- GLSL codegen that currently uses `get_math_libcall()` to call external functions like "sinf", "
  cosf", etc.
- Builtin registry system in `lp-glsl-compiler/src/backend/builtins/registry.rs` that supports both
  JIT and emulator modes
- Many failing builtin tests in `scripts/glsl-filetests.sh builtins`

## Goal

Refactor GLSL builtin math functions to call `lp-glsl-builtins` functions directly instead of using
GLSL intrinsics. We need both float and q32 implementations, with float being simple wrappers around
Rust std math functions.

## Scope

This plan focuses on **infrastructure** for builtin math functions. We will implement only `sin` and
`cos` as examples to prove the infrastructure works. Future plans can add more functions using the
same infrastructure.

## Questions

1. **Fixed-Point Math Library**: ✅ **DECIDED** - We'll port **libfixmath**'s implementation to Rust.
    - Uses Q16.16 format (perfect match for our q32)
    - Pure C, no dependencies, easy to port
    - Will use `FIXMATH_NO_CACHE` mode (no static memory)
    - See `00-library-analysis.md` for full analysis

   **Implementation details:**
    - ✅ **Which variant?** Accurate Taylor series (~2.1% accuracy)
    - ✅ **Operations**: Use Rust native operations for speed where possible:
        - Use `__lp_q32_mul` for fixed-point multiplies
        - Use Rust native division for compile-time constants (6, 120, 5040, etc.)
    - ✅ **Constants**: Use same constant values as libfixmath (e.g., `fix16_pi = 205887`) as `const`
      values
    - ✅ **Range reduction**: Use Rust's `%` operator for modulo operations

2. **Float Implementation Structure**: ✅ **DECIDED** - Use Cranelift native syscalls (TestCase
   external functions) for float trig functions.
    - Codegen continues to emit TestCase calls like "sinf", "cosf" (no change needed)
    - Q32 transform will convert these TestCase calls to `__lp_q32_*` calls
    - No need for `__lp_float_*` wrappers unless Cranelift doesn't support a function
    - Float and q32 are mutually exclusive per compilation (determined by target)

3. **Builtin Registry**: ✅ **DECIDED** - Add `Q32Sin` and `Q32Cos` to `BuiltinId` enum.
    - Follow same pattern as existing `Q32Div`, `Q32Mul`, `Q32Sqrt`
    - Add mapping table in q32 transform: `"sinf" -> "__lp_q32_sin"`, `"cosf" -> "__lp_q32_cos"`
    - Transform will detect TestCase calls to math functions and replace with `__lp_q32_*` calls

4. **Codegen Refactoring**: ✅ **DECIDED** - Stop using intrinsic GLSL calls, but don't remove the
   system yet.
    - `get_math_libcall()` already emits TestCase calls when `intrinsic-math` feature is disabled (
      current behavior)
    - Keep using `get_math_libcall()` - it will emit TestCase calls like "sinf", "cosf"
    - Q32 transform will convert these TestCase calls to `__lp_q32_*` calls via mapping table
    - Modify `convert_call()` in `converters/calls.rs` to detect TestCase calls to math functions
      and convert them
    - Don't remove intrinsic-math system code, just don't use it

5. **Test Structure**: ✅ **DECIDED** - Abstract sqrt test pattern into helper functions with
   tolerance parameter.
    - Use existing pattern from `sqrt.rs` as base (with `float_to_fixed` and `fixed_to_float`
      helpers)
    - Create helper functions that accept tolerance value as parameter
    - For trig functions (~2.1% accuracy), use ~2-3% tolerance

6. **Phase Structure**: ✅ **DECIDED** - Simplified structure:
    - 01-infrastructure: Helper functions, test utilities, registry updates
    - 02-implement-sin-cos: Port libfixmath, add to registry, add transform conversion
    - 03-testing: Testing and verification

7. **Acceptance Criteria**: ✅ **DECIDED** - `scripts/glsl-filetests.sh builtins/trig-sin.glsl` must
   pass.
    - This proves the infrastructure works
    - Cos should work similarly once sin is working
    - Edge cases: Handle basic range reduction, defer complex NaN/Inf handling if not critical

