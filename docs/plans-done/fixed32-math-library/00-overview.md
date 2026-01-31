# Plan: Q32 Math Library Infrastructure

## Overview

This plan establishes the infrastructure for q32 math builtin functions by implementing `sin` and
`cos` as proof-of-concept examples. The infrastructure will allow future plans to add more math
functions using the same pattern.

## Current State

- `lp-glsl-builtins` crate has q32 implementations for `div`, `mul`, `sqrt`
- GLSL codegen uses `get_math_libcall()` to emit TestCase external function calls (e.g., "sinf", "
  cosf")
- Q32 transform exists but doesn't convert math function calls yet
- Builtin registry has `Q32Div`, `Q32Mul`, `Q32Sqrt`

## Goal

Create infrastructure so that:

1. GLSL codegen emits TestCase calls for math functions (already works)
2. Q32 transform converts TestCase calls to `__lp_q32_*` calls
3. `__lp_q32_sin` and `__lp_q32_cos` are implemented using libfixmath's Taylor series
4. Tests verify the implementation works

## Approach

- **Reference Implementation**: Port libfixmath's Taylor series implementation (accurate version, ~
  2.1% accuracy)
- **Format**: Q16.16 fixed-point (matches our q32 format)
- **Operations**: Use Rust native operations where possible (native division for constants),
  `__lp_q32_mul` for multiplies
- **Float Functions**: Use Cranelift native syscalls (TestCase calls) - no `__lp_float_*` wrappers
  needed
- **Transform**: Add mapping table and conversion logic in `convert_call()` to detect TestCase math
  calls and convert them

## Acceptance Criteria

`scripts/glsl-filetests.sh builtins/trig-sin.glsl` must pass.

## Scope

This plan focuses on infrastructure. Only `sin` and `cos` are implemented as examples. Future plans
can add more functions using the same infrastructure.

