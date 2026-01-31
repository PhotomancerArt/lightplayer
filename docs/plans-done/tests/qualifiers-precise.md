# Plan: Create Comprehensive Precise Qualifier Tests

## Overview

Create a complete test suite for GLSL precise qualifier in `lightplayer/crates/lp-glsl-filetests/filetests/qualifiers/precise/` following the flat naming convention with prefixes. These tests will comprehensively cover the precise qualifier which ensures operations are done in their stated order and with operator consistency. These tests are expected to fail initially, serving as a specification for implementing precise qualifier support in the compiler.

## Directory Structure

Following the flat naming convention with prefixes, create tests in a single `qualifiers/precise/` directory:

```javascript
qualifiers/precise/
├── declare-inline.glsl            (precise in declaration)
├── declare-separate.glsl           (precise on separate line)
├── output-variables.glsl           (precise on output variables)
├── block-member.glsl               (precise on block members)
├── struct-member.glsl              (precise on struct members - recursive)
├── order-preservation.glsl          (operations in stated order)
├── operator-consistency.glsl        (operators treated consistently)
├── no-reassociation.glsl           (no reassociation allowed)
├── no-fma.glsl                     (no fused multiply-add)
├── commutativity-allowed.glsl      (commutativity allowed)
├── function-consumption.glsl       (consumed by precise l-value)
├── return-value.glsl               (return values not affected)
├── output-param.glsl               (output parameters not affected)
├── control-flow-unaffected.glsl    (control flow expressions unaffected)
├── intermediate-unaffected.glsl    (intermediate expressions unaffected)
├── type-scalar.glsl                (precise on scalar types)
├── type-vector.glsl                (precise on vector types)
├── type-matrix.glsl                (precise on matrix types)
├── type-struct.glsl                (precise on struct types)
├── nested-struct.glsl              (precise on nested structs)
└── edge-tessellation.glsl          (precise for tessellation cracking)
```

## Test File Patterns

Each test file should follow the pattern from other test suites:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

precise out vec4 position;

void test_precise_output() {
    precise vec4 temp = vec4(1.0) + vec4(2.0);
    position = temp;
    // Operations must be in stated order
}

// run: test_precise_output() == void
```

## Key Test Categories

### 1. Precise Declaration

**declare-inline.glsl**: Test precise in declaration
- `precise out vec4 position;` - inline declaration
- Precise as part of declaration

**declare-separate.glsl**: Test precise on separate line
- `out vec4 position;`
- `precise position;` - separate declaration
- Redeclaring existing variable

### 2. Precise on Variables

**output-variables.glsl**: Test precise on output variables
- Precise on output variables
- Output precision
- Various output types

**block-member.glsl**: Test precise on block members
- Precise on interface block members
- Block member precision
- Member-level precise

**struct-member.glsl**: Test precise on struct members (recursive)
- Precise on struct applies to all members
- Recursive application
- All members precise

### 3. Order Preservation

**order-preservation.glsl**: Test operations in stated order
- Operations done in stated order
- Order determined by precedence and parentheses
- Order preservation

**no-reassociation.glsl**: Test no reassociation allowed
- `a + (b + c)` cannot become `(a + b) + c`
- `a * (b * c)` cannot become `(a * b) * c`
- Reassociation prevented

**no-fma.glsl**: Test no fused multiply-add
- `a * b + c` cannot become `fma(a, b, c)`
- FMA transformation prevented
- Separate operations required

**commutativity-allowed.glsl**: Test commutativity allowed
- `a + b = b + a` allowed
- `a * b = b * a` allowed
- `a * b + c * d = b * a + c * d` allowed
- Commutative transformations allowed

### 4. Operator Consistency

**operator-consistency.glsl**: Test operators treated consistently
- Each operator always computed with same precision
- Operator consistency
- Precision consistency

### 5. Precise Consumption

**function-consumption.glsl**: Test consumed by precise l-value
- Expression affected if consumed by precise l-value
- Consumption requirement
- L-value must be precise

**return-value.glsl**: Test return values not affected
- Return values not affected by precise
- Function return precision
- Return value handling

**output-param.glsl**: Test output parameters not affected
- Output parameters not affected by precise
- Function parameter precision
- Parameter handling

**control-flow-unaffected.glsl**: Test control flow expressions unaffected
- Control flow expressions not affected
- If/while/for conditions
- Ternary conditions

**intermediate-unaffected.glsl**: Test intermediate expressions unaffected
- Intermediate expressions not affected
- Only final consumption matters
- Intermediate precision

### 6. Type Support

**type-scalar.glsl**: Test precise on scalar types
- float, int, uint, bool
- Scalar precision

**type-vector.glsl**: Test precise on vector types
- vec2, vec3, vec4
- ivec, uvec, bvec
- Vector precision

**type-matrix.glsl**: Test precise on matrix types
- mat2, mat3, mat4
- Matrix precision
- Matrix operations

**type-struct.glsl**: Test precise on struct types
- Struct precision
- All members precise
- Struct operations

**nested-struct.glsl**: Test precise on nested structs
- Nested struct precision
- Recursive application
- All nested members precise

### 7. Edge Cases

**edge-tessellation.glsl**: Test precise for tessellation cracking
- Precise prevents cracking in tessellation
- Tessellation coordinate consistency
- Subdivision consistency

## Implementation Notes

1. **Test Format**: Follow the exact format from other test suites with:
   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:
   - Precise declaration forms
   - Order preservation
   - Operator consistency
   - Reassociation prevention
   - FMA prevention
   - Consumption requirements
   - Type support

3. **Key Characteristics**:
   - Precise ensures stated order of operations
   - Operators treated consistently
   - No reassociation
   - No FMA transformation
   - Commutativity allowed
   - Only affects expressions consumed by precise l-values

4. **Expected Failures**: These tests are expected to fail initially, especially:
   - Precise qualifier parsing
   - Order preservation enforcement
   - Reassociation prevention
   - FMA prevention
   - Consumption tracking

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/globals/declare-out.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/functions/return-vector.glsl`
   - GLSL spec: `variables.adoc` - The Precise Qualifier (lines 6270-6418)

## Files to Create

Create 22 test files in the `qualifiers/precise/` directory structure above, with each file containing 3-10 test functions following the established pattern. All files use the prefix naming convention:

- `declare-*` for declaration forms
- `order-*`, `operator-*` for order/consistency
- `no-*` for prevented transformations
- `*-unaffected` for unaffected expressions
- `type-*` for type support
- `edge-*` for edge cases

## GLSL Spec References

- **variables.adoc**: The Precise Qualifier (lines 6270-6418)
- Key sections:
  - Precise qualifier syntax
  - Order preservation
  - Operator consistency
  - Reassociation prevention
  - FMA prevention
  - Consumption requirements
  - Tessellation applications






