# Questions for Remaining Builtins Plan

## Current State

- Framework is in place: q32 builtin infrastructure exists with auto-generation of boilerplate
- 24 tests passing, 40 tests failing
- Already implemented: trig functions (sin, cos, tan, asin, acos, atan, sinh, cosh, tanh, asinh, acosh, atanh), exp/log functions (exp, exp2, log, log2, pow), basic arithmetic (mul, div, sqrt)
- Reference libraries available: libfixmath, fpm, fr_math

## Questions

1. **Which functions need q32 implementations vs inline conversion?** ✅ ANSWERED
   - Floor/ceil/trunc are already converted inline in the transform
   - **Q32 function implementations needed:** fract, mod, sign, round, roundeven, fma, frexp, ldexp, modf, inversesqrt
   - **Inline conversion:** isinf, isnan (simple checks - always false for fixed-point since no NaN/Inf)
   - **Pure integer operations (no q32 needed):** Integer bit manipulation functions (bitcount, bitfieldextract, etc.) - these operate on int/uint types directly

2. **What's the priority/grouping for implementation?** ✅ ANSWERED
   - **Criteria for inline vs builtin:**
     - **Inline if:** < 10 instructions, no calls to other builtins, simple operations
     - **Builtin if:** Calls other builtins, complex algorithm, benefits from optimization
   - **Decision:**
     - **Inline:** fract (uses inline floor), sign (simple comparisons)
     - **Builtin:** mod (uses div builtin), round/roundeven (complex rounding), fma, frexp, ldexp, modf, inversesqrt
     - **Bit operations:** Evaluate complexity - simple (< 10 instructions) → inline, complex → builtin
   - **Grouping:** By complexity and dependencies

3. **Reference library usage:** ✅ ANSWERED
   - **Pattern:** Follow existing trig function patterns (sin.rs, exp.rs, log2.rs)
   - **Approach:**
     - Study algorithms from reference libraries (libfixmath primary, fpm secondary)
     - Port to Rust following existing patterns
     - Add comments citing source: "Algorithm ported from libfixmath" or similar
     - Use existing builtin functions (mul, div, etc.) where possible
     - Include tests using `test_q32_function_relative` helper
   - **Licensing:** All libraries have permissive licenses (MIT/BSD-like), safe to use

4. **Special cases and edge cases:**
   - **NaN/Inf:** Fixed-point doesn't have NaN/Inf - isinf/isnan always return false (inline conversion)
   - **Precision:** Follow existing patterns - use relative tolerance in tests (typically 3% for complex functions)
   - **Overflow/underflow:** Handle with saturation/clamping like existing functions (see exp.rs for overflow handling)

5. **Matrix and pack/unpack functions:** ✅ ANSWERED
   - **Matrix functions:** Already implemented in frontend codegen, but should be converted to builtins for consistency and optimization potential
     - Simple operations (compmult, transpose, outerProduct): Could stay inline but builtins would be cleaner
     - Complex operations (determinant, inverse): Should definitely be builtins
   - **Pack/unpack functions:** Not yet implemented. Should be implemented as builtins for consistency and optimization potential
   - **Decision:** Implement complex matrix operations (determinant, inverse) and all pack/unpack functions as builtins. Simple matrix operations can be converted later if needed.

6. **Testing strategy:** ✅ ANSWERED
   - **Pattern:** Follow existing test patterns (see sin.rs, exp.rs)
   - **Unit tests:** Yes, implement tests in each function file using `test_q32_function_relative`
   - **Tolerance:** Use 3% relative tolerance for complex functions, 1% for simpler ones (match existing patterns)
   - **Edge cases:** Test special cases (zero, one, negative, overflow) like existing functions do

