# Plan: Create Comprehensive uint Tests

## Overview

Create a complete test suite for scalar unsigned integer type `uint` in `lightplayer/crates/lp-glsl-filetests/filetests/uint/` following the organizational pattern used in `bool/`. These tests will comprehensively cover the GLSL unsigned integer scalar specification and are expected to fail initially, serving as a specification for implementing the rest of the compiler.

## Directory Structure

Following the flat naming convention with prefixes (like `bool/`), create tests in a single `uint/` directory:

```javascript
uint/
├── op-add.glsl              (uint + uint -> uint)
├── op-subtract.glsl         (uint - uint -> uint)
├── op-multiply.glsl         (uint * uint -> uint)
├── op-divide.glsl           (uint / uint -> uint, truncates)
├── op-modulo.glsl           (uint % uint -> uint)
├── op-equal.glsl            (uint == uint -> bool)
├── op-not-equal.glsl        (uint != uint -> bool)
├── op-less-than.glsl        (uint < uint -> bool)
├── op-greater-than.glsl     (uint > uint -> bool)
├── op-less-equal.glsl       (uint <= uint -> bool)
├── op-greater-equal.glsl    (uint >= uint -> bool)
├── op-unary-plus.glsl       (+uint -> uint)
├── op-increment-pre.glsl    (++uint -> uint, pre-increment)
├── op-increment-post.glsl   (uint++ -> uint, post-increment)
├── op-decrement-pre.glsl    (--uint -> uint, pre-decrement)
├── op-decrement-post.glsl   (uint-- -> uint, post-decrement)
├── from-bool.glsl           (uint(bool) - false -> 0u, true -> 1u)
├── from-int.glsl            (uint(int) - preserves value if non-negative)
├── from-float.glsl          (uint(float) - truncates fractional part)
├── from-uint.glsl           (uint(uint) - identity)
├── to-bool.glsl             (bool(uint) - 0u -> false, non-zero -> true)
├── to-int.glsl              (int(uint) - preserves value if in range)
├── to-float.glsl            (float(uint) - converts to float)
├── to-uvec.glsl             (uvec*(uint) - broadcast to vector components)
├── assign-simple.glsl       (uint = uint)
├── ctrl-if.glsl             (if (uint) - control flow with uint condition)
├── ctrl-while.glsl          (while (uint) - loop condition)
├── ctrl-for.glsl            (for (init; uint; update) - for loop condition)
├── ctrl-do-while.glsl       (do { } while (uint) - do-while condition)
├── ctrl-ternary.glsl        (uint ? expr1 : expr2 - ternary condition)
├── edge-zero.glsl           (uint(0u) edge cases)
├── edge-max.glsl            (max uint value: 4294967295u)
├── edge-wraparound.glsl     (overflow wraparound behavior)
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

uint test_uint_operation_name() {
    // Test implementation
    return result;
    // Should be expected_value
}

// run: test_uint_operation_name() == expected_value
```

## Key Test Categories

### 1. Arithmetic Operators

**op-add.glsl**: Test `+` operator
- `uint + uint` → `uint` (addition)
- Test with various values
- Test overflow behavior (wraparound, modulo 2^32)

**op-subtract.glsl**: Test `-` operator
- `uint - uint` → `uint` (subtraction)
- Test with various values
- Test underflow behavior (wraparound, modulo 2^32)

**op-multiply.glsl**: Test `*` operator
- `uint * uint` → `uint` (multiplication)
- Test with various values
- Test overflow behavior (wraparound, modulo 2^32)

**op-divide.glsl**: Test `/` operator
- `uint / uint` → `uint` (division, truncates)
- Test with various values
- Test division by zero (undefined behavior)

**op-modulo.glsl**: Test `%` operator
- `uint % uint` → `uint` (modulo)
- Test with various values
- Test modulo by zero (undefined behavior)

### 2. Comparison Operators

**op-equal.glsl**: Test `==` operator
- `uint == uint` → `bool`
- Test equality with same/different values

**op-not-equal.glsl**: Test `!=` operator
- `uint != uint` → `bool`
- Test inequality

**op-less-than.glsl**: Test `<` operator
- `uint < uint` → `bool`
- Test less than comparisons

**op-greater-than.glsl**: Test `>` operator
- `uint > uint` → `bool`
- Test greater than comparisons

**op-less-equal.glsl**: Test `<=` operator
- `uint <= uint` → `bool`
- Test less than or equal

**op-greater-equal.glsl**: Test `>=` operator
- `uint >= uint` → `bool`
- Test greater than or equal

### 3. Unary Operators

**op-unary-plus.glsl**: Test `+` unary operator
- `+uint` → `uint` (no-op for uint)

### 4. Increment/Decrement Operators

**op-increment-pre.glsl**: Test `++` pre-increment
- `++uint` → `uint` (increment and return new value)
- Must be on lvalue

**op-increment-post.glsl**: Test `++` post-increment
- `uint++` → `uint` (return old value, then increment)
- Must be on lvalue

**op-decrement-pre.glsl**: Test `--` pre-decrement
- `--uint` → `uint` (decrement and return new value)
- Must be on lvalue

**op-decrement-post.glsl**: Test `--` post-decrement
- `uint--` → `uint` (return old value, then decrement)
- Must be on lvalue

### 5. Constructors

**from-bool.glsl**: Test constructor from bool
- `uint(bool)` - converts bool to uint (false → 0u, true → 1u)

**from-int.glsl**: Test constructor from int
- `uint(int)` - preserves value if non-negative
- Negative values undefined behavior

**from-float.glsl**: Test constructor from float
- `uint(float)` - truncates fractional part toward zero
- Negative values and values outside uint range undefined behavior

**from-uint.glsl**: Test identity constructor
- `uint(uint)` - identity constructor
- Should preserve all values

### 6. Conversions

**to-bool.glsl**: Test conversion to bool
- `bool(uint)` - 0u → false, non-zero → true

**to-int.glsl**: Test conversion to int
- `int(uint)` - preserves value if in range
- Values > INT_MAX undefined behavior

**to-float.glsl**: Test conversion to float
- `float(uint)` - converts to float
- Test with large values (precision loss possible)

**to-uvec.glsl**: Test conversion to vectors
- `uvec2(uint)`, `uvec3(uint)`, `uvec4(uint)` - broadcast scalar to all components

### 7. Assignment

**assign-simple.glsl**: Test simple assignment
- `uint a = uint(...); uint b = a;` - assignment
- Verify independence (modifying one doesn't affect the other)
- Self-assignment: `uint = uint`

### 8. Control Flow

**ctrl-if.glsl**: Test `if` statements with uint
- `if (uint)` - condition converted to bool (0u → false, non-zero → true)
- Test with 0u, positive values

**ctrl-while.glsl**: Test `while` loops with uint
- `while (uint)` - loop condition

**ctrl-for.glsl**: Test `for` loops with uint
- `for (init; uint; update)` - for loop condition

**ctrl-do-while.glsl**: Test `do-while` loops with uint
- `do { } while (uint)` - do-while condition

**ctrl-ternary.glsl**: Test ternary operator with uint
- `uint ? expr1 : expr2` - ternary with uint condition

### 9. Edge Cases

**edge-zero.glsl**: Test edge case with zero
- `uint(0u)` patterns
- Division by zero, modulo by zero
- Zero as condition in control flow

**edge-max.glsl**: Test max uint value
- `4294967295u` (UINT_MAX)
- Operations at the maximum value
- Wraparound behavior

**edge-wraparound.glsl**: Test wraparound behavior
- Addition that exceeds UINT_MAX (wraps to 0)
- Subtraction that goes below 0 (wraps around)
- Multiplication wraparound

**edge-division-by-zero.glsl**: Test division by zero
- `uint / 0u` - undefined behavior
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
   - All built-in functions (builtinfunctions.adoc: unsigned integer functions)
   - Control flow requirements (statements.adoc: conditions convert to bool)

3. **Key Differences from bool**:
   - Arithmetic operations available (`+`, `-`, `*`, `/`, `%`)
   - Comparison operators available (`<`, `>`, `<=`, `>=`)
   - Increment/decrement operators work on lvalues
   - No logical operators (`&&`, `||`, `^^`, `!`) (those work on bool only)
   - No unary minus (unsigned values are always non-negative)
   - Control flow conditions: uint converts to bool (0u → false, non-zero → true)
   - Wraparound arithmetic (modulo 2^32) instead of overflow

4. **Expected Failures**: These tests are expected to fail initially, especially:
   - Some constructor forms (from int, from float)
   - Some conversion forms (to int, to vectors)
   - Increment/decrement operators
   - Control flow with uint conditions

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/bool/op-equal.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/bool/to-int.glsl`

## Files to Create

Create 25 test files in the flat `uint/` directory structure above, with each file containing 3-10 test functions following the bool pattern. All files use the prefix naming convention:

- `op-*` for operators (arithmetic, comparison, unary, increment/decrement)
- `from-*` for constructors (from-bool, from-int, from-float, from-uint)
- `to-*` for conversions (to-bool, to-int, to-float, to-uvec)
- `assign-*` for assignments (assign-simple)
- `ctrl-*` for control flow (if, while, for, do-while, ternary)
- `edge-*` for edge cases (zero, max, wraparound, division-by-zero)

## GLSL Spec References

- **operators.adoc**: Arithmetic operators (lines 580-700), Comparison operators (lines 700-885), Conversions (lines 908-1100)
- **builtinfunctions.adoc**: Common functions for unsigned integer types
