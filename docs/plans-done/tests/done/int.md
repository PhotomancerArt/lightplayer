# Plan: Create Comprehensive int Tests

## Overview

Create a complete test suite for scalar signed integer type `int` in `lightplayer/crates/lp-glsl-filetests/filetests/int/` following the organizational pattern used in `bool/`. These tests will comprehensively cover the GLSL signed integer scalar specification and are expected to fail initially, serving as a specification for implementing the rest of the compiler.

## Directory Structure

Following the flat naming convention with prefixes (like `bool/`), create tests in a single `int/` directory:

```javascript
int/
├── op-add.glsl              (int + int -> int)
├── op-subtract.glsl         (int - int -> int)
├── op-multiply.glsl         (int * int -> int)
├── op-divide.glsl           (int / int -> int, truncates)
├── op-modulo.glsl           (int % int -> int)
├── op-equal.glsl            (int == int -> bool)
├── op-not-equal.glsl        (int != int -> bool)
├── op-less-than.glsl        (int < int -> bool)
├── op-greater-than.glsl     (int > int -> bool)
├── op-less-equal.glsl       (int <= int -> bool)
├── op-greater-equal.glsl    (int >= int -> bool)
├── op-unary-plus.glsl       (+int -> int)
├── op-unary-minus.glsl      (-int -> int, negation)
├── op-increment-pre.glsl    (++int -> int, pre-increment)
├── op-increment-post.glsl   (int++ -> int, post-increment)
├── op-decrement-pre.glsl    (--int -> int, pre-decrement)
├── op-decrement-post.glsl   (int-- -> int, post-decrement)
├── from-bool.glsl           (int(bool) - false -> 0, true -> 1)
├── from-float.glsl          (int(float) - truncates fractional part)
├── from-uint.glsl           (int(uint) - preserves value if in range)
├── from-int.glsl            (int(int) - identity)
├── to-bool.glsl             (bool(int) - 0 -> false, non-zero -> true)
├── to-float.glsl            (float(int) - converts to float)
├── to-uint.glsl             (uint(int) - preserves value if non-negative)
├── to-ivec.glsl             (ivec*(int) - broadcast to vector components)
├── assign-simple.glsl       (int = int)
├── ctrl-if.glsl             (if (int) - control flow with int condition)
├── ctrl-while.glsl          (while (int) - loop condition)
├── ctrl-for.glsl            (for (init; int; update) - for loop condition)
├── ctrl-do-while.glsl       (do { } while (int) - do-while condition)
├── ctrl-ternary.glsl        (int ? expr1 : expr2 - ternary condition)
├── edge-zero.glsl           (int(0) edge cases)
├── edge-min-max.glsl        (min/max int values: -2147483648, 2147483647)
├── edge-overflow.glsl       (overflow behavior - implementation defined)
└── edge-division-by-zero.glsl (division by zero - undefined behavior)
```

## Test File Patterns

Each test file should follow the pattern from `bool/` tests:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

int test_int_operation_name() {
    // Test implementation
    return result;
    // Should be expected_value
}

// run: test_int_operation_name() == expected_value
```

## Key Test Categories

### 1. Arithmetic Operators

**op-add.glsl**: Test `+` operator
- `int + int` → `int` (addition)
- Test with positive/negative numbers
- Test overflow behavior (implementation-defined)

**op-subtract.glsl**: Test `-` operator
- `int - int` → `int` (subtraction)
- Test with positive/negative numbers
- Test underflow behavior

**op-multiply.glsl**: Test `*` operator
- `int * int` → `int` (multiplication)
- Test with positive/negative numbers
- Test overflow behavior

**op-divide.glsl**: Test `/` operator
- `int / int` → `int` (division, truncates toward zero)
- Test with positive/negative numbers
- Test division by zero (undefined behavior)
- Note: GLSL integer division always truncates toward zero (unlike C)

**op-modulo.glsl**: Test `%` operator
- `int % int` → `int` (modulo)
- Test with positive/negative numbers
- Test modulo by zero (undefined behavior)
- Sign of result follows dividend (unlike C)

### 2. Comparison Operators

**op-equal.glsl**: Test `==` operator
- `int == int` → `bool`
- Test equality with same/different values

**op-not-equal.glsl**: Test `!=` operator
- `int != int` → `bool`
- Test inequality

**op-less-than.glsl**: Test `<` operator
- `int < int` → `bool`
- Test less than comparisons

**op-greater-than.glsl**: Test `>` operator
- `int > int` → `bool`
- Test greater than comparisons

**op-less-equal.glsl**: Test `<=` operator
- `int <= int` → `bool`
- Test less than or equal

**op-greater-equal.glsl**: Test `>=` operator
- `int >= int` → `bool`
- Test greater than or equal

### 3. Unary Operators

**op-unary-plus.glsl**: Test `+` unary operator
- `+int` → `int` (no-op for int)

**op-unary-minus.glsl**: Test `-` unary operator
- `-int` → `int` (negation)
- Test with positive/negative values
- Test negation of min int value (undefined behavior)

### 4. Increment/Decrement Operators

**op-increment-pre.glsl**: Test `++` pre-increment
- `++int` → `int` (increment and return new value)
- Must be on lvalue

**op-increment-post.glsl**: Test `++` post-increment
- `int++` → `int` (return old value, then increment)
- Must be on lvalue

**op-decrement-pre.glsl**: Test `--` pre-decrement
- `--int` → `int` (decrement and return new value)
- Must be on lvalue

**op-decrement-post.glsl**: Test `--` post-decrement
- `int--` → `int` (return old value, then decrement)
- Must be on lvalue

### 5. Constructors

**from-bool.glsl**: Test constructor from bool
- `int(bool)` - converts bool to int (false → 0, true → 1)

**from-float.glsl**: Test constructor from float
- `int(float)` - truncates fractional part toward zero
- Test with positive/negative floats
- Test with NaN/Inf (undefined behavior)

**from-uint.glsl**: Test constructor from uint
- `int(uint)` - preserves value if in range
- Test with values that fit/don't fit in signed int

**from-int.glsl**: Test identity constructor
- `int(int)` - identity constructor
- Should preserve all values

### 6. Conversions

**to-bool.glsl**: Test conversion to bool
- `bool(int)` - 0 → false, non-zero → true

**to-float.glsl**: Test conversion to float
- `float(int)` - converts to float
- Test with large values (precision loss possible)

**to-uint.glsl**: Test conversion to uint
- `uint(int)` - preserves value if non-negative
- Negative values undefined behavior

**to-ivec.glsl**: Test conversion to vectors
- `ivec2(int)`, `ivec3(int)`, `ivec4(int)` - broadcast scalar to all components

### 7. Assignment

**assign-simple.glsl**: Test simple assignment
- `int a = int(...); int b = a;` - assignment
- Verify independence (modifying one doesn't affect the other)
- Self-assignment: `int = int`

### 8. Control Flow

**ctrl-if.glsl**: Test `if` statements with int
- `if (int)` - condition converted to bool (0 → false, non-zero → true)
- Test with 0, positive, negative values

**ctrl-while.glsl**: Test `while` loops with int
- `while (int)` - loop condition

**ctrl-for.glsl**: Test `for` loops with int
- `for (init; int; update)` - for loop condition

**ctrl-do-while.glsl**: Test `do-while` loops with int
- `do { } while (int)` - do-while condition

**ctrl-ternary.glsl**: Test ternary operator with int
- `int ? expr1 : expr2` - ternary with int condition

### 9. Edge Cases

**edge-zero.glsl**: Test edge case with zero
- `int(0)` patterns
- Division by zero, modulo by zero
- Zero as condition in control flow

**edge-min-max.glsl**: Test min/max int values
- `-2147483648` (INT_MIN) and `2147483647` (INT_MAX)
- Negation of INT_MIN (undefined behavior)
- Arithmetic operations near limits

**edge-overflow.glsl**: Test overflow behavior
- Addition/multiplication that exceeds INT_MAX
- Subtraction that goes below INT_MIN
- Implementation-defined behavior (test what implementation does)

**edge-division-by-zero.glsl**: Test division by zero
- `int / 0` - undefined behavior
- Test what implementation does (may crash or produce undefined result)

## Implementation Notes

1. **Test Format**: Follow the exact format from `bool/` tests with:
   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:
   - All operators from GLSL spec (operators.adoc)
   - All constructor forms (operators.adoc, constructors section)
   - All conversion forms (operators.adoc, conversion section)
   - All built-in functions (builtinfunctions.adoc: GenIType functions)
   - Control flow requirements (statements.adoc: conditions convert to bool)

3. **Key Differences from bool**:
   - Arithmetic operations available (`+`, `-`, `*`, `/`, `%`)
   - Comparison operators available (`<`, `>`, `<=`, `>=`)
   - Increment/decrement operators work on lvalues
   - No logical operators (`&&`, `||`, `^^`, `!`) (those work on bool only)
   - Control flow conditions: int converts to bool (0 → false, non-zero → true)

4. **Expected Failures**: These tests are expected to fail initially, especially:
   - Some constructor forms (from float, from uint)
   - Some conversion forms (to uint, to vectors)
   - Increment/decrement operators
   - Control flow with int conditions

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/bool/op-equal.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/math/int-add.glsl`

## Files to Create

Create 25 test files in the flat `int/` directory structure above, with each file containing 3-10 test functions following the bool pattern. All files use the prefix naming convention:

- `op-*` for operators (arithmetic, comparison, unary, increment/decrement)
- `from-*` for constructors (from-bool, from-float, from-uint, from-int)
- `to-*` for conversions (to-bool, to-float, to-uint, to-ivec)
- `assign-*` for assignments (assign-simple)
- `ctrl-*` for control flow (if, while, for, do-while, ternary)
- `edge-*` for edge cases (zero, min-max, overflow, division-by-zero)

## GLSL Spec References

- **operators.adoc**: Arithmetic operators (lines 580-700), Comparison operators (lines 700-885), Conversions (lines 908-1100)
- **builtinfunctions.adoc**: Common functions for `GenIType` (int, ivec*)
