# Plan: Q32 Math Library - Remaining Functions

## Overview

This plan implements the remaining q32 math builtin functions following the infrastructure established in the previous plan. We'll implement tan, inverse trig, exponential, hyperbolic, and power functions using reference implementations from libfixmath, fr_math, and fpm libraries.

## Current State

- ✅ Infrastructure for q32 math builtins (test helpers, registry, transform conversion)
- ✅ `sin` and `cos` implemented using libfixmath's Taylor series
- ✅ Transform conversion working for both TestCase and User function names
- ✅ Test infrastructure with tolerance support

## Goal

Implement the remaining q32 math functions:
- **Phase 3**: Tangent (tan) - simplest, depends on sin/cos
- **Phase 4**: Inverse trig (asin, acos, atan, atan2)
- **Phase 5**: Exponential base (exp, log, exp2, log2)
- **Phase 6**: Hyperbolic trig (sinh, cosh, tanh, asinh, acosh, atanh)
- **Phase 7**: Power (pow)

## Approach

- **Reference Implementations**: 
  - libfixmath for most functions (sin, cos, tan, asin, acos, atan, atan2, exp, log, log2)
  - fr_math for exp2 (pow2)
  - fpm for pow (uses exp2/log2)
  - Mathematical formulas for hyperbolic functions
- **Function Relationships**: Use relationships where reference libraries do (tan = sin/cos, acos = π/2 - asin, etc.)
- **Transform**: Extend to support 2-arg functions (atan2, pow)
- **Tests**: Source test cases from reference implementations, use 0.01 tolerance initially

## Acceptance Criteria

Each phase has its own acceptance criteria:
- Phase 3: `builtins/phases/02-basic-trig.glsl` passes (tan test)
- Phase 4: `builtins/phases/03-inverse-trig.glsl` passes
- Phase 5: `builtins/phases/05-exponential.glsl` passes (exp, log, exp2, log2 tests)
- Phase 6: `builtins/phases/04-hyperbolic-trig.glsl` passes
- Phase 7: `builtins/phases/05-exponential.glsl` passes (pow tests)

