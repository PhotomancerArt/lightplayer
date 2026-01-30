# Plan: Create Comprehensive Built-in Function Tests

## Overview

Create a complete test suite for GLSL built-in functions that are NOT covered by type-specific tests (e.g., vec2, vec4, etc.) in `lightplayer/crates/lp-glsl-filetests/filetests/builtins/` following the flat naming convention with prefixes. These tests will comprehensively cover math functions (trigonometry, exponential, common), matrix functions, integer/bit manipulation functions, and floating-point pack/unpack functions. These tests are expected to fail initially, serving as a specification for implementing built-in function support in the compiler.

## Directory Structure

Following the flat naming convention with prefixes, create tests in a single `builtins/` directory:

```javascript
builtins/
├── angle-radians.glsl               (radians() - convert degrees to radians)
├── angle-degrees.glsl               (degrees() - convert radians to degrees)
├── trig-sin.glsl                    (sin() - sine function)
├── trig-cos.glsl                    (cos() - cosine function)
├── trig-tan.glsl                    (tan() - tangent function)
├── trig-asin.glsl                   (asin() - arc sine)
├── trig-acos.glsl                   (acos() - arc cosine)
├── trig-atan.glsl                   (atan() - arc tangent, two variants)
├── trig-sinh.glsl                   (sinh() - hyperbolic sine)
├── trig-cosh.glsl                   (cosh() - hyperbolic cosine)
├── trig-tanh.glsl                   (tanh() - hyperbolic tangent)
├── trig-asinh.glsl                  (asinh() - arc hyperbolic sine)
├── trig-acosh.glsl                  (acosh() - arc hyperbolic cosine)
├── trig-atanh.glsl                  (atanh() - arc hyperbolic tangent)
├── exp-pow.glsl                     (pow() - x raised to y power)
├── exp-exp.glsl                     (exp() - natural exponentiation)
├── exp-log.glsl                     (log() - natural logarithm)
├── exp-exp2.glsl                    (exp2() - base 2 exponentiation)
├── exp-log2.glsl                    (log2() - base 2 logarithm)
├── exp-sqrt.glsl                    (sqrt() - square root)
├── exp-inversesqrt.glsl             (inversesqrt() - inverse square root)
├── common-sign.glsl                 (sign() - sign function)
├── common-floor.glsl                (floor() - floor function)
├── common-trunc.glsl                (trunc() - truncate toward zero)
├── common-round.glsl                (round() - round to nearest)
├── common-roundeven.glsl            (roundEven() - round to nearest even)
├── common-ceil.glsl                (ceil() - ceiling function)
├── common-fract.glsl               (fract() - fractional part)
├── common-mod.glsl                  (mod() - modulus)
├── common-modf.glsl                (modf() - mod with integer part)
├── common-isnan.glsl               (isnan() - check for NaN)
├── common-isinf.glsl               (isinf() - check for infinity)
├── common-floatbitstoint.glsl      (floatBitsToInt() - bit cast)
├── common-intbitstofloat.glsl      (intBitsToFloat() - bit cast)
├── common-fma.glsl                 (fma() - fused multiply-add)
├── common-frexp.glsl               (frexp() - extract mantissa and exponent)
├── common-ldexp.glsl               (ldexp() - scale by power of 2)
├── pack-unorm.glsl                 (packUnorm2x16, packUnorm4x8)
├── unpack-unorm.glsl               (unpackUnorm2x16, unpackUnorm4x8)
├── pack-half.glsl                  (packHalf2x16)
├── unpack-half.glsl                (unpackHalf2x16)
├── pack-double.glsl                (packDouble2x32)
├── unpack-double.glsl              (unpackDouble2x32)
├── matrix-compmult.glsl            (matrixCompMult() - component-wise multiply)
├── matrix-outerproduct.glsl        (outerProduct() - outer product)
├── matrix-transpose.glsl           (transpose() - matrix transpose)
├── matrix-determinant.glsl         (determinant() - matrix determinant)
├── matrix-inverse.glsl             (inverse() - matrix inverse)
├── integer-uaddcarry.glsl          (uaddCarry() - unsigned add with carry)
├── integer-usubborrow.glsl         (usubBorrow() - unsigned subtract with borrow)
├── integer-umulextended.glsl       (umulExtended() - unsigned multiply extended)
├── integer-imulextended.glsl       (imulExtended() - signed multiply extended)
├── integer-bitfieldextract.glsl    (bitfieldExtract() - extract bitfield)
├── integer-bitfieldinsert.glsl    (bitfieldInsert() - insert bitfield)
├── integer-bitfieldreverse.glsl    (bitfieldReverse() - reverse bits)
├── integer-bitcount.glsl           (bitCount() - count set bits)
├── integer-findlsb.glsl            (findLSB() - find least significant bit)
├── integer-findmsb.glsl            (findMSB() - find most significant bit)
├── edge-trig-domain.glsl            (trigonometric domain errors)
├── edge-exp-domain.glsl             (exponential domain errors)
├── edge-nan-inf-propagation.glsl    (NaN and Inf propagation)
├── edge-precision.glsl              (precision and rounding)
└── edge-component-wise.glsl         (component-wise operation verification)
```

## Test File Patterns

Each test file should follow the pattern from other test suites:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

float test_builtin_function_name() {
    float x = 1.0;
    float result = sin(x);
    return result;
    // Should be approximately sin(1.0)
}

// run: test_builtin_function_name() ~= 0.8414709848
```

## Key Test Categories

### 1. Angle and Trigonometry Functions

**angle-radians.glsl**: Test `radians()` function
- `radians(degrees)` - converts degrees to radians
- Formula: `(π / 180) * degrees`
- Test with scalar and vector inputs (float, vec2, vec3, vec4)
- Test with 0, 90, 180, 360 degrees

**angle-degrees.glsl**: Test `degrees()` function
- `degrees(radians)` - converts radians to degrees
- Formula: `(180 / π) * radians`
- Test with scalar and vector inputs
- Test with 0, π/2, π, 2π radians

**trig-sin.glsl**: Test `sin()` function
- `sin(angle)` - standard trigonometric sine
- Test with scalar and vector inputs
- Test with 0, π/2, π, 3π/2, 2π
- Test with negative angles

**trig-cos.glsl**: Test `cos()` function
- `cos(angle)` - standard trigonometric cosine
- Test with scalar and vector inputs
- Test with 0, π/2, π, 3π/2, 2π
- Test with negative angles

**trig-tan.glsl**: Test `tan()` function
- `tan(angle)` - standard trigonometric tangent
- Test with scalar and vector inputs
- Test with 0, π/4, π/2 (undefined), π
- Test with negative angles

**trig-asin.glsl**: Test `asin()` function
- `asin(x)` - arc sine, returns angle whose sine is x
- Range: [-π/2, π/2]
- Undefined if |x| > 1
- Test with -1, 0, 1, and values outside domain

**trig-acos.glsl**: Test `acos()` function
- `acos(x)` - arc cosine, returns angle whose cosine is x
- Range: [0, π]
- Undefined if |x| > 1
- Test with -1, 0, 1, and values outside domain

**trig-atan.glsl**: Test `atan()` function
- `atan(y_over_x)` - arc tangent (single argument)
- Range: [-π/2, π/2]
- `atan(y, x)` - arc tangent (two arguments, determines quadrant)
- Range: [-π, π]
- Undefined if both x and y are 0 (two-arg version)
- Test both variants with various inputs

**trig-sinh.glsl**: Test `sinh()` function
- `sinh(x)` - hyperbolic sine: (e^x - e^-x) / 2
- Test with scalar and vector inputs
- Test with 0, positive, negative values

**trig-cosh.glsl**: Test `cosh()` function
- `cosh(x)` - hyperbolic cosine: (e^x + e^-x) / 2
- Test with scalar and vector inputs
- Test with 0, positive, negative values

**trig-tanh.glsl**: Test `tanh()` function
- `tanh(x)` - hyperbolic tangent: sinh(x) / cosh(x)
- Test with scalar and vector inputs
- Test with 0, positive, negative values

**trig-asinh.glsl**: Test `asinh()` function
- `asinh(x)` - arc hyperbolic sine, inverse of sinh
- Test with scalar and vector inputs

**trig-acosh.glsl**: Test `acosh()` function
- `acosh(x)` - arc hyperbolic cosine, inverse of cosh
- Undefined if x < 1
- Test with x < 1 (undefined), x = 1, x > 1

**trig-atanh.glsl**: Test `atanh()` function
- `atanh(x)` - arc hyperbolic tangent, inverse of tanh
- Undefined if |x| >= 1
- Test with |x| < 1, |x| = 1, |x| > 1

### 2. Exponential Functions

**exp-pow.glsl**: Test `pow()` function
- `pow(x, y)` - returns x raised to y power (x^y)
- Undefined if x < 0
- Undefined if x = 0 and y <= 0
- Test with positive x, y
- Test with x = 0, y > 0
- Test with x < 0 (undefined)
- Test with x = 0, y <= 0 (undefined)

**exp-exp.glsl**: Test `exp()` function
- `exp(x)` - returns natural exponentiation (e^x)
- Test with scalar and vector inputs
- Test with 0, positive, negative values
- Test with large values (overflow)

**exp-log.glsl**: Test `log()` function
- `log(x)` - returns natural logarithm of x
- Undefined if x <= 0
- Test with x > 0
- Test with x = 0 (undefined)
- Test with x < 0 (undefined)

**exp-exp2.glsl**: Test `exp2()` function
- `exp2(x)` - returns 2 raised to x power (2^x)
- Test with scalar and vector inputs
- Test with 0, positive, negative values

**exp-log2.glsl**: Test `log2()` function
- `log2(x)` - returns base 2 logarithm of x
- Undefined if x <= 0
- Test with x > 0
- Test with x = 0 (undefined)
- Test with x < 0 (undefined)

**exp-sqrt.glsl**: Test `sqrt()` function
- `sqrt(x)` - returns square root of x
- Undefined if x < 0
- Test with x >= 0
- Test with x < 0 (undefined)

**exp-inversesqrt.glsl**: Test `inversesqrt()` function
- `inversesqrt(x)` - returns 1 / sqrt(x)
- Undefined if x <= 0
- Test with x > 0
- Test with x = 0 (undefined)
- Test with x < 0 (undefined)

### 3. Common Functions (Not Covered in Type Tests)

**common-sign.glsl**: Test `sign()` function
- `sign(x)` - returns 1.0 if x > 0, 0.0 if x = 0, -1.0 if x < 0
- Works with float, int, vec*, ivec*
- Test with positive, zero, negative values

**common-floor.glsl**: Test `floor()` function
- `floor(x)` - returns nearest integer <= x
- Test with scalar and vector inputs
- Test with positive, negative, fractional values

**common-trunc.glsl**: Test `trunc()` function
- `trunc(x)` - truncates toward zero
- Test with scalar and vector inputs
- Test with positive, negative values

**common-round.glsl**: Test `round()` function
- `round(x)` - rounds to nearest integer
- 0.5 rounds in implementation-defined direction
- Test with scalar and vector inputs

**common-roundeven.glsl**: Test `roundEven()` function
- `roundEven(x)` - rounds to nearest even integer
- 0.5 rounds toward even (3.5 → 4.0, 4.5 → 4.0)
- Test with scalar and vector inputs

**common-ceil.glsl**: Test `ceil()` function
- `ceil(x)` - returns nearest integer >= x
- Test with scalar and vector inputs
- Test with positive, negative, fractional values

**common-fract.glsl**: Test `fract()` function
- `fract(x)` - returns x - floor(x)
- Test with scalar and vector inputs
- Test with positive, negative values

**common-mod.glsl**: Test `mod()` function
- `mod(x, y)` - returns x - y * floor(x / y)
- Test with scalar and vector inputs
- Test with various x, y combinations

**common-modf.glsl**: Test `modf()` function
- `modf(x, out i)` - returns fractional part, sets i to integer part
- Both return value and i have same sign as x
- Test with scalar and vector inputs

**common-isnan.glsl**: Test `isnan()` function
- `isnan(x)` - returns true if x is NaN
- Test with NaN, Inf, normal values
- Test with scalar and vector inputs

**common-isinf.glsl**: Test `isinf()` function
- `isinf(x)` - returns true if x is infinity
- Test with NaN, Inf, normal values
- Test with scalar and vector inputs

**common-floatbitstoint.glsl**: Test `floatBitsToInt()` function
- `floatBitsToInt(x)` - bit cast float to int
- Test with various float values
- Test with NaN, Inf

**common-intbitstofloat.glsl**: Test `intBitsToFloat()` function
- `intBitsToFloat(x)` - bit cast int to float
- Test with various int values

**common-fma.glsl**: Test `fma()` function
- `fma(a, b, c)` - fused multiply-add: a * b + c
- More accurate than separate multiply and add
- Test with scalar and vector inputs

**common-frexp.glsl**: Test `frexp()` function
- `frexp(x, out exp)` - splits x into mantissa and exponent
- Returns mantissa, sets exp to exponent
- Test with scalar and vector inputs

**common-ldexp.glsl**: Test `ldexp()` function
- `ldexp(x, exp)` - returns x * 2^exp
- Test with scalar and vector inputs

### 4. Floating-Point Pack and Unpack Functions

**pack-unorm.glsl**: Test `packUnorm*()` functions
- `packUnorm2x16(vec2)` - pack 2 floats to uint
- `packUnorm4x8(vec4)` - pack 4 floats to uint
- Test with 0.0, 1.0, and intermediate values

**unpack-unorm.glsl**: Test `unpackUnorm*()` functions
- `unpackUnorm2x16(uint)` - unpack uint to vec2
- `unpackUnorm4x8(uint)` - unpack uint to vec4
- Test round-trip with pack functions

**pack-half.glsl**: Test `packHalf2x16()` function
- `packHalf2x16(vec2)` - pack 2 floats to uint (half precision)
- Test with various float values

**unpack-half.glsl**: Test `unpackHalf2x16()` function
- `unpackHalf2x16(uint)` - unpack uint to vec2 (half precision)
- Test round-trip with pack function

**pack-double.glsl**: Test `packDouble2x32()` function
- `packDouble2x32(dvec2)` - pack 2 doubles to uvec2
- Test with various double values

**unpack-double.glsl**: Test `unpackDouble2x32()` function
- `unpackDouble2x32(uvec2)` - unpack uvec2 to dvec2
- Test round-trip with pack function

### 5. Matrix Functions

**matrix-compmult.glsl**: Test `matrixCompMult()` function
- `matrixCompMult(mat, mat)` - component-wise matrix multiply
- Test with mat2, mat3, mat4
- Test with various matrix values

**matrix-outerproduct.glsl**: Test `outerProduct()` function
- `outerProduct(vec, vec)` - outer product of two vectors
- Returns matrix: column vector * row vector
- Test with vec2, vec3, vec4 combinations
- Test various matrix sizes (mat2, mat3, mat4, mat2x3, etc.)

**matrix-transpose.glsl**: Test `transpose()` function
- `transpose(mat)` - returns transpose of matrix
- Test with mat2, mat3, mat4
- Test with non-square matrices (mat2x3, mat3x2, etc.)

**matrix-determinant.glsl**: Test `determinant()` function
- `determinant(mat)` - returns determinant of matrix
- Test with mat2, mat3, mat4
- Test with singular matrices (determinant = 0)
- Test with identity matrices (determinant = 1)

**matrix-inverse.glsl**: Test `inverse()` function
- `inverse(mat)` - returns inverse of matrix
- Undefined if matrix is singular or poorly-conditioned
- Test with mat2, mat3, mat4
- Test with identity matrices
- Test with singular matrices (undefined)

### 6. Integer Functions

**integer-uaddcarry.glsl**: Test `uaddCarry()` function
- `uaddCarry(x, y, out carry)` - unsigned add with carry
- Returns sum modulo 2^32
- Sets carry to 0 if sum < 2^32, 1 otherwise
- Test with uint, uvec2, uvec3, uvec4

**integer-usubborrow.glsl**: Test `usubBorrow()` function
- `usubBorrow(x, y, out borrow)` - unsigned subtract with borrow
- Sets borrow to 0 if x >= y, 1 otherwise
- Test with uint, uvec2, uvec3, uvec4

**integer-umulextended.glsl**: Test `umulExtended()` function
- `umulExtended(x, y, out msb, out lsb)` - unsigned multiply extended
- Produces 64-bit result
- lsb = 32 least significant bits
- msb = 32 most significant bits
- Test with uint, uvec2, uvec3, uvec4

**integer-imulextended.glsl**: Test `imulExtended()` function
- `imulExtended(x, y, out msb, out lsb)` - signed multiply extended
- Produces 64-bit result
- Test with int, ivec2, ivec3, ivec4

**integer-bitfieldextract.glsl**: Test `bitfieldExtract()` function
- `bitfieldExtract(value, offset, bits)` - extract bitfield
- Extracts bits [offset, offset + bits - 1]
- Undefined if offset < 0, bits < 0, or offset + bits > 32
- Test with int, uint, ivec*, uvec*

**integer-bitfieldinsert.glsl**: Test `bitfieldInsert()` function
- `bitfieldInsert(base, insert, offset, bits)` - insert bitfield
- Inserts bits [0, bits-1] of insert into base at [offset, offset+bits-1]
- Undefined if offset < 0, bits < 0, or offset + bits > 32
- Test with int, uint, ivec*, uvec*

**integer-bitfieldreverse.glsl**: Test `bitfieldReverse()` function
- `bitfieldReverse(value)` - reverses bits of value
- Test with int, uint, ivec*, uvec*

**integer-bitcount.glsl**: Test `bitCount()` function
- `bitCount(value)` - returns number of one bits
- Test with int, uint, ivec*, uvec*
- Test with 0, all ones, various patterns

**integer-findlsb.glsl**: Test `findLSB()` function
- `findLSB(value)` - returns bit number of least significant one bit
- Returns -1 if value is 0
- Test with int, uint, ivec*, uvec*

**integer-findmsb.glsl**: Test `findMSB()` function
- `findMSB(value)` - returns bit number of most significant bit
- For positive: most significant one bit
- For negative: most significant zero bit
- Returns -1 if value is 0 or -1
- Test with int, uint, ivec*, uvec*

### 7. Edge Cases

**edge-trig-domain.glsl**: Test trigonometric domain errors
- asin/acos with |x| > 1 (undefined)
- atan(y, x) with x = 0, y = 0 (undefined)
- acosh with x < 1 (undefined)
- atanh with |x| >= 1 (undefined)

**edge-exp-domain.glsl**: Test exponential domain errors
- pow with x < 0 (undefined)
- pow with x = 0, y <= 0 (undefined)
- log/log2 with x <= 0 (undefined)
- sqrt with x < 0 (undefined)
- inversesqrt with x <= 0 (undefined)

**edge-nan-inf-propagation.glsl**: Test NaN and Inf propagation
- NaN propagation through operations
- Inf propagation through operations
- Operations producing NaN/Inf
- isnan/isinf with various inputs

**edge-precision.glsl**: Test precision and rounding
- Rounding behavior (round, roundEven)
- Precision loss in operations
- Approximate equality testing

**edge-component-wise.glsl**: Test component-wise operations
- Verify operations are truly component-wise
- Test with vectors having different component values
- Verify independence of components

## Implementation Notes

1. **Test Format**: Follow the exact format from other test suites with:
   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results (use `~=` for approximate equality)
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:
   - All math functions (trigonometry, exponential, common)
   - All matrix functions
   - All integer/bit manipulation functions
   - All pack/unpack functions
   - Domain errors and undefined behavior
   - NaN and Inf handling
   - Component-wise operation verification

3. **Key Differences from Type Tests**:
   - These tests focus on function behavior, not type operations
   - Tests cover functions that work across multiple types (genFType, genIType, etc.)
   - Tests verify mathematical correctness, not just type compatibility
   - Tests cover edge cases specific to each function

4. **Expected Failures**: These tests are expected to fail initially, especially:
   - Trigonometric functions
   - Exponential functions
   - Matrix functions (determinant, inverse)
   - Integer/bit manipulation functions
   - Pack/unpack functions
   - Domain error handling
   - NaN/Inf propagation

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/float/op-add.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/fn-length.glsl`
   - GLSL spec: `builtinfunctions.adoc` - All function sections

## Files to Create

Create 65 test files in the flat `builtins/` directory structure above, with each file containing 3-10 test functions following the established pattern. All files use the prefix naming convention:

- `angle-*` for angle conversion functions
- `trig-*` for trigonometric functions
- `exp-*` for exponential functions
- `common-*` for common functions
- `pack-*` and `unpack-*` for pack/unpack functions
- `matrix-*` for matrix functions
- `integer-*` for integer/bit manipulation functions
- `edge-*` for edge cases

## GLSL Spec References

- **builtinfunctions.adoc**: 
  - Angle and Trigonometry Functions (lines 123-267)
  - Exponential Functions (lines 270-350)
  - Common Functions (lines 352-820)
  - Floating-Point Pack and Unpack Functions (lines 820-968)
  - Matrix Functions (lines 1119-1201)
  - Integer Functions (lines 1317-1468)
- Key sections:
  - Function signatures and overloads
  - Mathematical formulas and behavior
  - Domain restrictions and undefined behavior
  - Component-wise vs vector operations
  - Precision requirements

