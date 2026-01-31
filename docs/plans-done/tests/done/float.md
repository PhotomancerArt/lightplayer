# Plan: Create Comprehensive float Tests

## Overview

Create a complete test suite for scalar floating-point type `float` in `lightplayer/crates/lp-glsl-filetests/filetests/float/` following the organizational pattern used in `bool/`. These tests will comprehensively cover the GLSL single-precision floating-point scalar specification and are expected to fail initially, serving as a specification for implementing the rest of the compiler.

## Directory Structure

Following the flat naming convention with prefixes (like `bool/`), create tests in a single `float/` directory:

```javascript
float/
├── op-add.glsl              (float + float -> float)
├── op-subtract.glsl         (float - float -> float)
├── op-multiply.glsl         (float * float -> float)
├── op-divide.glsl           (float / float -> float)
├── op-equal.glsl            (float == float -> bool)
├── op-not-equal.glsl        (float != float -> bool)
├── op-less-than.glsl        (float < float -> bool)
├── op-greater-than.glsl     (float > float -> bool)
├── op-less-equal.glsl       (float <= float -> bool)
├── op-greater-equal.glsl    (float >= float -> bool)
├── op-unary-plus.glsl       (+float -> float)
├── op-unary-minus.glsl      (-float -> float, negation)
├── op-increment-pre.glsl    (++float -> float, pre-increment)
├── op-increment-post.glsl   (float++ -> float, post-increment)
├── op-decrement-pre.glsl    (--float -> float, pre-decrement)
├── op-decrement-post.glsl   (float-- -> float, post-decrement)
├── from-bool.glsl           (float(bool) - false -> 0.0, true -> 1.0)
├── from-int.glsl            (float(int) - converts to float)
├── from-uint.glsl           (float(uint) - converts to float)
├── from-float.glsl          (float(float) - identity)
├── to-bool.glsl             (bool(float) - 0.0 -> false, non-zero -> true)
├── to-int.glsl              (int(float) - truncates fractional part)
├── to-uint.glsl             (uint(float) - truncates fractional part)
├── to-vec.glsl              (vec*(float) - broadcast to vector components)
├── assign-simple.glsl       (float = float)
├── ctrl-if.glsl             (if (float) - control flow with float condition)
├── ctrl-while.glsl          (while (float) - loop condition)
├── ctrl-for.glsl            (for (init; float; update) - for loop condition)
├── ctrl-do-while.glsl       (do { } while (float) - do-while condition)
├── ctrl-ternary.glsl        (float ? expr1 : expr2 - ternary condition)
├── edge-zero.glsl           (float(0.0) edge cases)
├── edge-nan-inf.glsl        (NaN, +Inf, -Inf values)
├── edge-precision.glsl      (floating-point precision and rounding)
├── edge-small-values.glsl   (very small float values)
└── edge-large-values.glsl   (very large float values)
```

## Test File Patterns

Each test file should follow the pattern from `bool/` tests:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

float test_float_operation_name() {
    // Test implementation
    return result;
    // Should be expected_value
}

// run: test_float_operation_name() ~= expected_value
```

Note: Use `~=` for approximate equality due to floating-point precision issues.

## Key Test Categories

### 1. Arithmetic Operators

**op-add.glsl**: Test `+` operator

- `float + float` → `float` (addition)
- Test with positive/negative numbers
- Test with NaN/Inf (special IEEE 754 behavior)

**op-subtract.glsl**: Test `-` operator

- `float - float` → `float` (subtraction)
- Test with positive/negative numbers
- Test subtraction involving NaN/Inf

**op-multiply.glsl**: Test `*` operator

- `float * float` → `float` (multiplication)
- Test with positive/negative numbers
- Test multiplication by zero, Inf, NaN

**op-divide.glsl**: Test `/` operator

- `float / float` → `float` (division)
- Test with positive/negative numbers
- Test division by zero (produces Inf)
- Test division involving NaN/Inf

### 2. Comparison Operators

**op-equal.glsl**: Test `==` operator

- `float == float` → `bool`
- Test equality with same/different values
- Note: NaN == NaN is false (IEEE 754)

**op-not-equal.glsl**: Test `!=` operator

- `float != float` → `bool`
- Test inequality

**op-less-than.glsl**: Test `<` operator

- `float < float` → `bool`
- Test less than comparisons
- NaN comparisons always false

**op-greater-than.glsl**: Test `>` operator

- `float > float` → `bool`
- Test greater than comparisons

**op-less-equal.glsl**: Test `<=` operator

- `float <= float` → `bool`
- Test less than or equal

**op-greater-equal.glsl**: Test `>=` operator

- `float >= float` → `bool`
- Test greater than or equal

### 3. Unary Operators

**op-unary-plus.glsl**: Test `+` unary operator

- `+float` → `float` (no-op for float)

**op-unary-minus.glsl**: Test `-` unary operator

- `-float` → `float` (negation)
- Test with positive/negative values, NaN, Inf

### 4. Increment/Decrement Operators

**op-increment-pre.glsl**: Test `++` pre-increment

- `++float` → `float` (increment and return new value)
- Must be on lvalue

**op-increment-post.glsl**: Test `++` post-increment

- `float++` → `float` (return old value, then increment)
- Must be on lvalue

**op-decrement-pre.glsl**: Test `--` pre-decrement

- `--float` → `float` (decrement and return new value)
- Must be on lvalue

**op-decrement-post.glsl**: Test `--` post-decrement

- `float--` → `float` (return old value, then decrement)
- Must be on lvalue

### 5. Constructors

**from-bool.glsl**: Test constructor from bool

- `float(bool)` - converts bool to float (false → 0.0, true → 1.0)

**from-int.glsl**: Test constructor from int

- `float(int)` - converts int to float
- Test with large int values (precision loss possible)

**from-uint.glsl**: Test constructor from uint

- `float(uint)` - converts uint to float
- Test with large uint values (precision loss possible)

**from-float.glsl**: Test identity constructor

- `float(float)` - identity constructor
- Should preserve all values including NaN/Inf

### 6. Conversions

**to-bool.glsl**: Test conversion to bool

- `bool(float)` - 0.0 → false, non-zero → true
- Test with NaN (NaN → true, as it's non-zero)

**to-int.glsl**: Test conversion to int

- `int(float)` - truncates fractional part toward zero
- Test with positive/negative floats
- Test with values outside int range (undefined behavior)

**to-uint.glsl**: Test conversion to uint

- `uint(float)` - truncates fractional part toward zero
- Negative values and values outside uint range undefined behavior

**to-vec.glsl**: Test conversion to vectors

- `vec2(float)`, `vec3(float)`, `vec4(float)` - broadcast scalar to all components

### 7. Assignment

**assign-simple.glsl**: Test simple assignment

- `float a = float(...); float b = a;` - assignment
- Verify independence (modifying one doesn't affect the other)
- Self-assignment: `float = float`

### 8. Control Flow

**ctrl-if.glsl**: Test `if` statements with float

- `if (float)` - condition converted to bool (0.0 → false, non-zero → true)
- Test with 0.0, positive, negative values, NaN

**ctrl-while.glsl**: Test `while` loops with float

- `while (float)` - loop condition

**ctrl-for.glsl**: Test `for` loops with float

- `for (init; float; update)` - for loop condition

**ctrl-do-while.glsl**: Test `do-while` loops with float

- `do { } while (float)` - do-while condition

**ctrl-ternary.glsl**: Test ternary operator with float

- `float ? expr1 : expr2` - ternary with float condition

### 9. Edge Cases

**edge-zero.glsl**: Test edge case with zero

- `float(0.0)` patterns
- Positive zero vs negative zero
- Division by zero, multiplication by zero

**edge-nan-inf.glsl**: Test NaN and Inf values

- `float('inf')`, `float('-inf')` - positive/negative infinity
- NaN propagation through operations
- NaN comparisons (always false)
- NaN as condition in control flow

**edge-precision.glsl**: Test floating-point precision

- Loss of precision with large integers
- Rounding behavior
- Approximate equality testing

**edge-small-values.glsl**: Test very small float values

- Values near FLT_MIN
- Underflow behavior
- Denormalized numbers

**edge-large-values.glsl**: Test very large float values

- Values near FLT_MAX
- Overflow behavior
- Infinity from operations

## Implementation Notes

1. **Test Format**: Follow the exact format from `bool/` tests with:

   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results (use `~=` for approximate equality)
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:

   - All operators from GLSL spec (operators.adoc)
   - All constructor forms (operators.adoc, constructors section)
   - All conversion forms (operators.adoc, conversion section)
   - All built-in functions (builtinfunctions.adoc: GenFType functions, exponential, angle/trigonometry)
   - Control flow requirements (statements.adoc: conditions convert to bool)

3. **Key Differences from bool**:

   - Arithmetic operations available (`+`, `-`, `*`, `/`)
   - Comparison operators available (`<`, `>`, `<=`, `>=`)
   - Increment/decrement operators work on lvalues
   - No logical operators (`&&`, `||`, `^^`, `!`) (those work on bool only)
   - Control flow conditions: float converts to bool (0.0 → false, non-zero → true)
   - IEEE 754 floating-point semantics (NaN, Inf, precision issues)

4. **Expected Failures**: These tests are expected to fail initially, especially:

   - Some constructor forms (from int, from uint)
   - Some conversion forms (to int, to uint, to vectors)
   - Increment/decrement operators
   - Control flow with float conditions
   - NaN/Inf handling

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/bool/op-equal.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/math/float-add.glsl`

## Files to Create

Create 26 test files in the flat `float/` directory structure above, with each file containing 3-10 test functions following the bool pattern. All files use the prefix naming convention:

- `op-*` for operators (arithmetic, comparison, unary, increment/decrement)
- `from-*` for constructors (from-bool, from-int, from-uint, from-float)
- `to-*` for conversions (to-bool, to-int, to-uint, to-vec)
- `assign-*` for assignments (assign-simple)
- `ctrl-*` for control flow (if, while, for, do-while, ternary)
- `edge-*` for edge cases (zero, nan-inf, precision, small-values, large-values)

## GLSL Spec References

- **operators.adoc**: Arithmetic operators (lines 580-700), Comparison operators (lines 700-885), Conversions (lines 908-1100)
- **builtinfunctions.adoc**: Common functions for `GenFType` (float, vec\*), Exponential functions, Angle/Trigonometry functions
