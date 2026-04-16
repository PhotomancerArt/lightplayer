# Remaining Builtins Plan - Overview

## Goal
Complete implementation of all remaining GLSL builtin functions to achieve 100% test pass rate (currently 24/64 tests passing).

## Current State
- **Framework:** Q32 builtin infrastructure with auto-generated boilerplate is in place
- **Tests:** 24 passing, 40 failing
- **Already implemented:** 
  - Trig functions (sin, cos, tan, asin, acos, atan, sinh, cosh, tanh, asinh, acosh, atanh)
  - Exp/log functions (exp, exp2, log, log2, pow)
  - Basic arithmetic (mul, div, sqrt)
- **Reference libraries:** libfixmath, fpm, fr_math available

## Approach
- **Inline vs Builtin:** Use 10-instruction threshold
  - Simple operations (< 10 instructions) → inline conversion in transform
  - Complex operations or those calling other builtins → q32 builtin functions
- **Implementation pattern:** Follow existing trig function patterns (sin.rs, exp.rs, log2.rs)
  - Port algorithms from reference libraries (libfixmath primary, fpm secondary)
  - Cite sources in comments
  - Use existing builtin functions where possible
  - Include unit tests with appropriate tolerance

## Phases
1. **Inline Conversion Functions** - fract, sign, isinf/isnan ✅
2. **Common Builtin Functions** - mod, round, roundeven ✅
3. **Exponential Functions** - inversesqrt ✅
4. **Advanced Common Functions** - fma ✅, ldexp ✅, frexp ⏸️ (deferred), modf ⏸️ (deferred)
5. **Matrix Complex Functions** - determinant, inverse ✅
6. **Pack/Unpack Functions** - all pack/unpack variants ⏸️ (deferred - frontend codegen)
7. **Integer Bit Functions** - evaluate and implement case-by-case ⏸️ (deferred - frontend codegen)
8. **Frontend Codegen** - missing frontend implementations ⏸️ (deferred - separate from q32 work)
9. **Integration Testing and Cleanup** - final verification

## Success Criteria
- All 64 builtin tests pass
- No warnings
- Code follows existing patterns
- All code is clean, readable, and formatted

