# Phases for Remaining Builtins Plan

## Phase 1: Inline Conversion Functions
Implement inline conversions for simple functions (< 10 instructions):
- fract (uses inline floor) - tests: `builtins/common-fract.glsl`
- sign (simple comparisons) - tests: `builtins/common-sign.glsl`
- isinf/isnan (always false for fixed-point) - tests: `builtins/common-isinf.glsl`, `builtins/common-isnan.glsl`

**Success criteria:**
- All inline conversion functions implemented in transform
- Tests pass for fract, sign, isinf, isnan
- Code compiles without warnings

## Phase 2: Common Builtin Functions
Implement q32 builtins for common functions:
- mod (uses div builtin) - tests: `builtins/common-mod.glsl`
- round (complex rounding algorithm) - tests: `builtins/common-round.glsl`
- roundeven (round to nearest even) - tests: `builtins/common-roundeven.glsl`

**Success criteria:**
- All functions implemented following existing patterns
- Unit tests pass with appropriate tolerance
- Functions registered in builtin generator
- Code compiles without warnings

## Phase 3: Exponential Functions
Implement inversesqrt builtin:
- inversesqrt (uses sqrt builtin: 1/sqrt(x)) - tests: `builtins/exp-inversesqrt.glsl`

**Success criteria:**
- Function implemented following existing patterns
- Unit tests pass
- Function registered in builtin generator
- Code compiles without warnings

## Phase 4: Advanced Common Functions
Implement complex common functions:
- fma (fused multiply-add) - tests: `builtins/common-fma.glsl` ✅
- ldexp (scale by power of 2) - tests: `builtins/common-ldexp.glsl` ✅
- frexp (extract mantissa and exponent) - tests: `builtins/common-frexp.glsl` ⏸️ **DEFERRED** - requires output parameter support (Phase 8)
- modf (split into integer and fractional parts) - tests: `builtins/common-modf.glsl` ⏸️ **DEFERRED** - requires output parameter support (Phase 8)

**Success criteria:**
- All functions implemented following existing patterns
- Unit tests pass with appropriate tolerance
- Functions registered in builtin generator
- Code compiles without warnings

## Phase 5: Matrix Complex Functions
Implement matrix builtins for complex operations:
- determinant (2x2, 3x3, 4x4) - tests: `builtins/matrix-determinant.glsl`
- inverse (2x2, 3x3, 4x4) - tests: `builtins/matrix-inverse.glsl`

**Success criteria:**
- Functions implemented following existing patterns
- Unit tests pass for all matrix sizes
- Functions registered in builtin generator
- Code compiles without warnings

## Phase 6: Pack/Unpack Functions ⏸️ **DEFERRED**
Implement pack/unpack builtins:
- packHalf2x16, packDouble2x32, packUnorm4x8 - tests: `builtins/pack-half.glsl`, `builtins/pack-double.glsl`, `builtins/pack-unorm.glsl`
- unpackHalf2x16, unpackDouble2x32, unpackUnorm4x8 - tests: `builtins/unpack-half.glsl`, `builtins/unpack-double.glsl`, `builtins/unpack-unorm.glsl`

**Status:** Deferred - requires frontend codegen for bit manipulation (not q32 math)

**Success criteria:**
- All pack/unpack functions implemented
- Unit tests pass
- Functions registered in builtin generator
- Code compiles without warnings

## Phase 7: Integer Bit Functions ⏸️ **DEFERRED**
Evaluate complexity and implement integer bit manipulation functions:
- Evaluate: bitCount, findLSB, findMSB, bitfieldReverse, imulextended, uaddcarry, umulextended, usubborrow
- Implement as inline or builtin based on complexity (< 10 instructions = inline, else builtin)
- Implement bitfieldExtract and bitfieldInsert as inline (already determined < 10 instructions)
- Tests: `builtins/integer-bitcount.glsl`, `builtins/integer-bitfieldextract.glsl`, `builtins/integer-bitfieldinsert.glsl`, `builtins/integer-bitfieldreverse.glsl`, `builtins/integer-findlsb.glsl`, `builtins/integer-findmsb.glsl`, `builtins/integer-imulextended.glsl`, `builtins/integer-uaddcarry.glsl`, `builtins/integer-umulextended.glsl`, `builtins/integer-usubborrow.glsl`

**Status:** Deferred - requires frontend codegen for pure integer operations (not q32 math)

**Success criteria:**
- All functions evaluated and implemented appropriately
- Unit tests pass
- Functions registered appropriately (inline or builtin)
- Code compiles without warnings

## Phase 8: Frontend Codegen for Missing Functions ⏸️ **DEFERRED**
Implement frontend codegen for functions that don't need q32:
- Integer bit functions that are pure integer operations (tests referenced in Phase 7)
- floatBitsToInt, intBitsToFloat - tests: `builtins/common-floatbitstoint.glsl`, `builtins/common-intbitstofloat.glsl`
- Output parameter support for frexp/modf
- Any other missing frontend implementations

**Status:** Deferred - frontend codegen work separate from q32 math library

**Success criteria:**
- All frontend codegen functions implemented
- Tests pass
- Code compiles without warnings

## Phase 9: Integration Testing and Cleanup
- Run all builtin tests and verify they pass
- Fix any remaining issues
- Remove temporary code, TODOs, debug prints
- Fix all warnings
- Ensure all code is clean and readable
- Format code with `cargo +nightly fmt`

**Success criteria:**
- All 64 builtin tests pass (24 currently passing + 40 failing)
- No warnings
- Code is clean and formatted
- All temporary code removed

