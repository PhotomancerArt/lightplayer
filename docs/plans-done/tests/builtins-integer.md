# Plan: Create Comprehensive Integer/Bit Function Tests

## Overview

Create a complete test suite for GLSL integer and bit manipulation functions in `lightplayer/crates/lp-glsl-filetests/filetests/builtins/integer/` following the flat naming convention with prefixes. These tests will cover the remaining integer/bit functions not yet covered in existing tests (uaddCarry and usubBorrow already exist). These tests are expected to fail initially, serving as a specification for implementing these built-in functions.

## Directory Structure

Following the flat naming convention with prefixes, create tests in a single `builtins/integer/` directory:

```javascript
builtins/integer/
├── mul-extended-unsigned.glsl    (umulExtended() - unsigned multiply extended)
├── mul-extended-signed.glsl      (imulExtended() - signed multiply extended)
├── bitfield-extract-int.glsl     (bitfieldExtract() with int)
├── bitfield-extract-uint.glsl    (bitfieldExtract() with uint)
├── bitfield-extract-vector.glsl  (bitfieldExtract() with vectors)
├── bitfield-extract-edge.glsl    (bitfieldExtract() edge cases)
├── bitfield-insert-int.glsl      (bitfieldInsert() with int)
├── bitfield-insert-uint.glsl     (bitfieldInsert() with uint)
├── bitfield-insert-vector.glsl   (bitfieldInsert() with vectors)
├── bitfield-insert-edge.glsl     (bitfieldInsert() edge cases)
├── bitfield-reverse-int.glsl     (bitfieldReverse() with int)
├── bitfield-reverse-uint.glsl    (bitfieldReverse() with uint)
├── bitfield-reverse-vector.glsl  (bitfieldReverse() with vectors)
├── bitcount-int.glsl             (bitCount() with int)
├── bitcount-uint.glsl            (bitCount() with uint)
├── bitcount-vector.glsl          (bitCount() with vectors)
├── findlsb-int.glsl              (findLSB() with int)
├── findlsb-uint.glsl             (findLSB() with uint)
├── findlsb-vector.glsl           (findLSB() with vectors)
├── findlsb-zero.glsl             (findLSB() with zero - returns -1)
├── findmsb-int-positive.glsl     (findMSB() with positive int)
├── findmsb-int-negative.glsl     (findMSB() with negative int)
├── findmsb-uint.glsl             (findMSB() with uint)
├── findmsb-vector.glsl           (findMSB() with vectors)
├── findmsb-edge.glsl             (findMSB() edge cases - zero, -1)
└── edge-precision.glsl            (precision qualifiers on results)
```

## Test File Patterns

Each test file should follow the pattern from other test suites:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

uint test_umul_extended_basic() {
    uint x = 0x10000u;
    uint y = 0x10000u;
    uint msb, lsb;
    umulExtended(x, y, msb, lsb);
    return lsb;
    // Should be 0 (32 least significant bits of 0x100000000)
}

// run: test_umul_extended_basic() == 0u
```

## Key Test Categories

### 1. Multiply Extended Functions

**mul-extended-unsigned.glsl**: Test `umulExtended()` function

- `umulExtended(x, y, out msb, out lsb)` - unsigned multiply extended
- Produces 64-bit result
- lsb = 32 least significant bits
- msb = 32 most significant bits
- Test with uint, uvec2, uvec3, uvec4
- Test with various values (small, large, overflow)

**mul-extended-signed.glsl**: Test `imulExtended()` function

- `imulExtended(x, y, out msb, out lsb)` - signed multiply extended
- Produces 64-bit result
- Test with int, ivec2, ivec3, ivec4
- Test with positive, negative values
- Test with overflow

### 2. Bitfield Extract Functions

**bitfield-extract-int.glsl**: Test `bitfieldExtract()` with int

- `bitfieldExtract(value, offset, bits)` - extract bitfield
- Extracts bits [offset, offset + bits - 1]
- Signed extension for int
- Test with various offsets and bit counts

**bitfield-extract-uint.glsl**: Test `bitfieldExtract()` with uint

- Unsigned extension (zero-fill)
- Test with various offsets and bit counts

**bitfield-extract-vector.glsl**: Test `bitfieldExtract()` with vectors

- Component-wise operation
- Single offset/bits pair shared for all components
- Test with ivec2, ivec3, ivec4, uvec2, uvec3, uvec4

**bitfield-extract-edge.glsl**: Test `bitfieldExtract()` edge cases

- offset < 0 - undefined
- bits < 0 - undefined
- offset + bits > 32 - undefined
- bits == 0 - returns 0
- Test boundary conditions

### 3. Bitfield Insert Functions

**bitfield-insert-int.glsl**: Test `bitfieldInsert()` with int

- `bitfieldInsert(base, insert, offset, bits)` - insert bitfield
- Inserts bits [0, bits-1] of insert into base at [offset, offset+bits-1]
- Test with various offsets and bit counts

**bitfield-insert-uint.glsl**: Test `bitfieldInsert()` with uint

- Same as int but with unsigned values
- Test with various offsets and bit counts

**bitfield-insert-vector.glsl**: Test `bitfieldInsert()` with vectors

- Component-wise operation
- Single offset/bits pair shared for all components
- Test with ivec2, ivec3, ivec4, uvec2, uvec3, uvec4

**bitfield-insert-edge.glsl**: Test `bitfieldInsert()` edge cases

- offset < 0 - undefined
- bits < 0 - undefined
- offset + bits > 32 - undefined
- bits == 0 - returns base
- Test boundary conditions

### 4. Bitfield Reverse Functions

**bitfield-reverse-int.glsl**: Test `bitfieldReverse()` with int

- `bitfieldReverse(value)` - reverses bits
- Bit n of result from bit (bits-1)-n of value
- Test with int, ivec2, ivec3, ivec4

**bitfield-reverse-uint.glsl**: Test `bitfieldReverse()` with uint

- Same as int but with unsigned values
- Test with uint, uvec2, uvec3, uvec4

**bitfield-reverse-vector.glsl**: Test `bitfieldReverse()` with vectors

- Component-wise operation
- Test with various vector types

### 5. Bit Count Functions

**bitcount-int.glsl**: Test `bitCount()` with int

- `bitCount(value)` - returns number of one bits
- Test with int, ivec2, ivec3, ivec4
- Test with 0, all ones, various patterns

**bitcount-uint.glsl**: Test `bitCount()` with uint

- Same as int but with unsigned values
- Test with uint, uvec2, uvec3, uvec4

**bitcount-vector.glsl**: Test `bitCount()` with vectors

- Component-wise operation
- Test with various vector types

### 6. Find LSB Functions

**findlsb-int.glsl**: Test `findLSB()` with int

- `findLSB(value)` - returns bit number of least significant one bit
- Returns -1 if value is 0
- Test with int, ivec2, ivec3, ivec4

**findlsb-uint.glsl**: Test `findLSB()` with uint

- Same as int but with unsigned values
- Test with uint, uvec2, uvec3, uvec4

**findlsb-vector.glsl**: Test `findLSB()` with vectors

- Component-wise operation
- Test with various vector types

**findlsb-zero.glsl**: Test `findLSB()` with zero

- Returns -1 for zero
- Test with 0, vec with zero components

### 7. Find MSB Functions

**findmsb-int-positive.glsl**: Test `findMSB()` with positive int

- `findMSB(value)` - returns bit number of most significant bit
- For positive: most significant one bit
- Test with int, ivec2, ivec3, ivec4

**findmsb-int-negative.glsl**: Test `findMSB()` with negative int

- For negative: most significant zero bit
- Test with negative values

**findmsb-uint.glsl**: Test `findMSB()` with uint

- For positive: most significant one bit
- Test with uint, uvec2, uvec3, uvec4

**findmsb-vector.glsl**: Test `findMSB()` with vectors

- Component-wise operation
- Test with various vector types

**findmsb-edge.glsl**: Test `findMSB()` edge cases

- Returns -1 if value is 0 or -1
- Test boundary conditions

### 8. Edge Cases

**edge-precision.glsl**: Test precision qualifiers on results

- Results have precision qualifiers (lowp for some)
- Precision handling

## Implementation Notes

1. **Test Format**: Follow the exact format from other test suites with:

   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:

   - All integer/bit manipulation functions
   - Scalar and vector versions
   - Edge cases (undefined behavior, boundary conditions)
   - Precision qualifiers
   - Out parameters (for umulExtended, imulExtended)

3. **Key Characteristics**:

   - These functions operate on integers/unsigned integers
   - Many operate component-wise on vectors
   - Some have undefined behavior for invalid parameters
   - Some return -1 for special cases (findLSB, findMSB)

4. **Expected Failures**: These tests are expected to fail initially, especially:

   - umulExtended/imulExtended with out parameters
   - Bitfield operations
   - Bit manipulation functions
   - Precision qualifier handling

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/builtins/integer-uaddcarry.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/builtins/integer-usubborrow.glsl`
   - GLSL spec: `builtinfunctions.adoc` - Integer Functions (lines 1317-1468)

## Files to Create

Create 25 test files in the `builtins/integer/` directory structure above, with each file containing 3-10 test functions following the established pattern. All files use the prefix naming convention:

- `mul-extended-*` for multiply extended functions
- `bitfield-extract-*` for bitfield extract
- `bitfield-insert-*` for bitfield insert
- `bitfield-reverse-*` for bitfield reverse
- `bitcount-*` for bit count
- `findlsb-*` for find LSB
- `findmsb-*` for find MSB
- `edge-*` for edge cases

## GLSL Spec References

- **builtinfunctions.adoc**: Integer Functions (lines 1317-1468)
- Key sections:
  - Multiply extended functions
  - Bitfield extract/insert
  - Bitfield reverse
  - Bit count
  - Find LSB/MSB
  - Precision qualifiers
  - Undefined behavior conditions





