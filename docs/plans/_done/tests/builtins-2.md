# Plan: Document Existing Built-in Function Tests

## Overview

This plan documents the existing built-in function tests in `lightplayer/crates/lp-glsl-filetests/filetests/builtins/`. These tests cover math functions (trigonometry, exponential, common), matrix functions, integer/bit manipulation functions, and floating-point pack/unpack functions.

## Existing Test Structure

The builtin tests are organized in a flat structure with 50+ test files:

```
builtins/
├── angle-degrees.glsl
├── angle-radians.glsl
├── common-ceil.glsl
├── common-floatbitstoint.glsl
├── common-floor.glsl
├── common-fma.glsl
├── common-fract.glsl
├── common-frexp.glsl
├── common-intbitstofloat.glsl
├── common-isinf.glsl
├── common-isnan.glsl
├── common-ldexp.glsl
├── common-mod.glsl
├── common-modf.glsl
├── common-round.glsl
├── common-roundeven.glsl
├── common-sign.glsl
├── common-trunc.glsl
├── exp-exp.glsl
├── exp-exp2.glsl
├── exp-inversesqrt.glsl
├── exp-log.glsl
├── exp-log2.glsl
├── exp-pow.glsl
├── exp-sqrt.glsl
├── integer-uaddcarry.glsl
├── integer-usubborrow.glsl
├── matrix-compmult.glsl
├── matrix-determinant.glsl
├── matrix-inverse.glsl
├── matrix-outerproduct.glsl
├── matrix-transpose.glsl
├── pack-double.glsl
├── pack-half.glsl
├── pack-unorm.glsl
├── trig-acos.glsl
├── trig-acosh.glsl
├── trig-asin.glsl
├── trig-asinh.glsl
├── trig-atan.glsl
├── trig-atanh.glsl
├── trig-cos.glsl
├── trig-cosh.glsl
├── trig-sin.glsl
├── trig-sinh.glsl
├── trig-tan.glsl
├── trig-tanh.glsl
├── unpack-double.glsl
├── unpack-half.glsl
└── unpack-unorm.glsl
```

## Test Coverage

### Angle Functions

**angle-radians.glsl**: `radians()` function
- Converts degrees to radians
- Formula: (π / 180) * degrees
- Scalar and vector inputs

**angle-degrees.glsl**: `degrees()` function
- Converts radians to degrees
- Formula: (180 / π) * radians
- Scalar and vector inputs

### Trigonometric Functions

**trig-sin.glsl**: `sin()` function
- Standard trigonometric sine
- Scalar and vector inputs

**trig-cos.glsl**: `cos()` function
- Standard trigonometric cosine
- Scalar and vector inputs

**trig-tan.glsl**: `tan()` function
- Standard trigonometric tangent
- Scalar and vector inputs

**trig-asin.glsl**: `asin()` function
- Arc sine
- Range: [-π/2, π/2]
- Undefined if |x| > 1

**trig-acos.glsl**: `acos()` function
- Arc cosine
- Range: [0, π]
- Undefined if |x| > 1

**trig-atan.glsl**: `atan()` function
- Arc tangent (single and two argument variants)
- Range: [-π/2, π/2] or [-π, π]

**trig-sinh.glsl**: `sinh()` function
- Hyperbolic sine
- Formula: (e^x - e^-x) / 2

**trig-cosh.glsl**: `cosh()` function
- Hyperbolic cosine
- Formula: (e^x + e^-x) / 2

**trig-tanh.glsl**: `tanh()` function
- Hyperbolic tangent
- Formula: sinh(x) / cosh(x)

**trig-asinh.glsl**: `asinh()` function
- Arc hyperbolic sine
- Inverse of sinh

**trig-acosh.glsl**: `acosh()` function
- Arc hyperbolic cosine
- Inverse of cosh
- Undefined if x < 1

**trig-atanh.glsl**: `atanh()` function
- Arc hyperbolic tangent
- Inverse of tanh
- Undefined if |x| >= 1

### Exponential Functions

**exp-pow.glsl**: `pow()` function
- x raised to y power (x^y)
- Undefined if x < 0
- Undefined if x = 0 and y <= 0

**exp-exp.glsl**: `exp()` function
- Natural exponentiation (e^x)
- Scalar and vector inputs

**exp-log.glsl**: `log()` function
- Natural logarithm
- Undefined if x <= 0

**exp-exp2.glsl**: `exp2()` function
- Base 2 exponentiation (2^x)
- Scalar and vector inputs

**exp-log2.glsl**: `log2()` function
- Base 2 logarithm
- Undefined if x <= 0

**exp-sqrt.glsl**: `sqrt()` function
- Square root
- Undefined if x < 0

**exp-inversesqrt.glsl**: `inversesqrt()` function
- Inverse square root (1 / sqrt(x))
- Undefined if x <= 0

### Common Functions

**common-sign.glsl**: `sign()` function
- Returns 1.0 if x > 0, 0.0 if x = 0, -1.0 if x < 0
- Works with float, int, vec*, ivec*

**common-floor.glsl**: `floor()` function
- Nearest integer <= x
- Scalar and vector inputs

**common-trunc.glsl**: `trunc()` function
- Truncates toward zero
- Scalar and vector inputs

**common-round.glsl**: `round()` function
- Rounds to nearest integer
- 0.5 rounds in implementation-defined direction

**common-roundeven.glsl**: `roundEven()` function
- Rounds to nearest even integer
- 0.5 rounds toward even

**common-ceil.glsl**: `ceil()` function
- Nearest integer >= x
- Scalar and vector inputs

**common-fract.glsl**: `fract()` function
- Returns x - floor(x)
- Scalar and vector inputs

**common-mod.glsl**: `mod()` function
- Returns x - y * floor(x / y)
- Scalar and vector inputs

**common-modf.glsl**: `modf()` function
- Returns fractional part, sets out parameter to integer part
- Both have same sign as x

**common-isnan.glsl**: `isnan()` function
- Returns true if x is NaN
- Scalar and vector inputs

**common-isinf.glsl**: `isinf()` function
- Returns true if x is infinity
- Scalar and vector inputs

**common-floatbitstoint.glsl**: `floatBitsToInt()` function
- Bit cast float to int
- Various float values

**common-intbitstofloat.glsl**: `intBitsToFloat()` function
- Bit cast int to float
- Various int values

**common-fma.glsl**: `fma()` function
- Fused multiply-add: a * b + c
- More accurate than separate operations

**common-frexp.glsl**: `frexp()` function
- Splits x into mantissa and exponent
- Returns mantissa, sets exp to exponent

**common-ldexp.glsl**: `ldexp()` function
- Returns x * 2^exp
- Scalar and vector inputs

### Matrix Functions

**matrix-compmult.glsl**: `matrixCompMult()` function
- Component-wise matrix multiply
- mat2, mat3, mat4

**matrix-outerproduct.glsl**: `outerProduct()` function
- Outer product of two vectors
- Returns matrix: column vector * row vector
- Various matrix sizes

**matrix-transpose.glsl**: `transpose()` function
- Returns transpose of matrix
- mat2, mat3, mat4
- Non-square matrices

**matrix-determinant.glsl**: `determinant()` function
- Returns determinant of matrix
- mat2, mat3, mat4
- Singular matrices

**matrix-inverse.glsl**: `inverse()` function
- Returns inverse of matrix
- Undefined if singular or poorly-conditioned
- mat2, mat3, mat4

### Integer Functions

**integer-uaddcarry.glsl**: `uaddCarry()` function
- Unsigned add with carry
- Returns sum modulo 2^32
- Sets carry to 0 or 1

**integer-usubborrow.glsl**: `usubBorrow()` function
- Unsigned subtract with borrow
- Sets borrow to 0 or 1

### Pack/Unpack Functions

**pack-unorm.glsl**: `packUnorm*()` functions
- packUnorm2x16(vec2)
- packUnorm4x8(vec4)
- Pack floats to uint

**unpack-unorm.glsl**: `unpackUnorm*()` functions
- unpackUnorm2x16(uint)
- unpackUnorm4x8(uint)
- Unpack uint to floats

**pack-half.glsl**: `packHalf2x16()` function
- Pack 2 floats to uint (half precision)

**unpack-half.glsl**: `unpackHalf2x16()` function
- Unpack uint to vec2 (half precision)

**pack-double.glsl**: `packDouble2x32()` function
- Pack 2 doubles to uvec2

**unpack-double.glsl**: `unpackDouble2x32()` function
- Unpack uvec2 to dvec2

## Missing Coverage

The following built-in functions are NOT yet covered:

1. **Integer/Bit Functions (Partial)**:
   - `umulExtended()` - unsigned multiply extended
   - `imulExtended()` - signed multiply extended
   - `bitfieldExtract()` - extract bitfield
   - `bitfieldInsert()` - insert bitfield
   - `bitfieldReverse()` - reverse bits
   - `bitCount()` - count set bits
   - `findLSB()` - find least significant bit
   - `findMSB()` - find most significant bit

2. **Texture Functions** - See `textures.md`

3. **Image Functions** - See `builtins-images.md`

4. **Atomic Functions** - See `builtins-atomics.md`

5. **Geometry Shader Functions** - See `builtins-geometry.md`

6. **Fragment Processing Functions** - See `builtins-fragment.md`

7. **Noise Functions** - See `builtins-noise.md`

8. **Barrier Functions** - See `builtins-barriers.md`

9. **Subpass Functions** - See `builtins-subpass.md`

## GLSL Spec References

- **builtinfunctions.adoc**: 
  - Angle and Trigonometry Functions (lines 123-267)
  - Exponential Functions (lines 270-350)
  - Common Functions (lines 352-820)
  - Floating-Point Pack and Unpack Functions (lines 820-968)
  - Matrix Functions (lines 1119-1201)
  - Integer Functions (lines 1317-1468)






