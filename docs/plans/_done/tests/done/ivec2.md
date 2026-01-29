# Plan: Create Comprehensive ivec2 Tests

## Overview

Create a complete test suite for integer vector type `ivec2` in `lightplayer/crates/lp-glsl-filetests/filetests/ivec2/` following the organizational pattern used in `vec4/` and `bool/`. These tests will comprehensively cover the GLSL signed integer vector specification for `ivec2` and are expected to fail initially, serving as a specification for implementing the rest of the compiler.

## Directory Structure

Following the flat naming convention with prefixes (like `vec4/` and `bool/`), create tests in a single `ivec2/` directory:

```javascript
ivec2/
├── op-add.glsl              (ivec2 + ivec2 -> ivec2, component-wise)
├── op-subtract.glsl         (ivec2 - ivec2 -> ivec2, component-wise)
├── op-multiply.glsl         (ivec2 * ivec2 -> ivec2, component-wise)
├── op-divide.glsl           (ivec2 / ivec2 -> ivec2, component-wise, truncates)
├── op-modulo.glsl           (ivec2 % ivec2 -> ivec2, component-wise)
├── op-equal.glsl            (ivec2 == ivec2 -> bool, aggregate comparison)
├── op-not-equal.glsl        (ivec2 != ivec2 -> bool, aggregate comparison)
├── op-unary-minus.glsl      (-ivec2 -> ivec2, component-wise negation)
├── op-increment-pre.glsl    (++ivec2 -> ivec2, pre-increment)
├── op-increment-post.glsl   (ivec2++ -> ivec2, post-increment)
├── op-decrement-pre.glsl    (--ivec2 -> ivec2, pre-decrement)
├── op-decrement-post.glsl   (ivec2-- -> ivec2, post-decrement)
├── fn-less-than.glsl        (lessThan(ivec2, ivec2) -> bvec2)
├── fn-greater-than.glsl     (greaterThan(ivec2, ivec2) -> bvec2)
├── fn-less-equal.glsl       (lessThanEqual(ivec2, ivec2) -> bvec2)
├── fn-greater-equal.glsl    (greaterThanEqual(ivec2, ivec2) -> bvec2)
├── fn-equal.glsl            (equal(ivec2, ivec2) -> bvec2, component-wise)
├── fn-not-equal.glsl        (notEqual(ivec2, ivec2) -> bvec2, component-wise)
├── fn-min.glsl              (min(ivec2, ivec2) -> ivec2, component-wise)
├── fn-max.glsl              (max(ivec2, ivec2) -> ivec2, component-wise)
├── fn-clamp.glsl            (clamp(ivec2, ivec2, ivec2) -> ivec2, component-wise)
├── fn-abs.glsl              (abs(ivec2) -> ivec2, component-wise)
├── from-scalar.glsl         (ivec2(int) - broadcast)
├── from-scalars.glsl        (ivec2(int, int))
├── from-shortening.glsl     (ivec2(ivec3), ivec2(ivec4) - shortening)
├── from-ivec.glsl           (ivec2(ivec2) - identity)
├── from-mixed.glsl          (ivec2(bool, bool), ivec2(float, float) - conversions)
├── from-vectors.glsl        (ivec2(ivec2) - identity)
├── to-int.glsl              (int(ivec2) - extract first component)
├── to-float.glsl            (float(ivec2) - extract first component)
├── to-bool.glsl             (bool(ivec2) - extract first component)
├── to-uvec.glsl             (uvec2(ivec2) - component-wise conversion)
├── to-vec.glsl              (vec2(ivec2) - component-wise conversion)
├── assign-simple.glsl       (ivec2 = ivec2)
├── assign-element.glsl      (ivec2.x = int, ivec2[0] = int - single component)
├── assign-swizzle.glsl      (ivec2.xy = ivec2 - multi-component swizzle)
├── access-array.glsl        (ivec2[0], ivec2[1])
├── access-component.glsl    (ivec2.x, ivec2.y)
├── access-swizzle.glsl      (ivec2.xy, ivec2.yx, ivec2.xx, etc.)
├── ctrl-if.glsl             (if (any(bvec_from_comparison)) - control flow)
├── ctrl-while.glsl          (while (any(bvec_from_comparison)))
├── ctrl-for.glsl            (for (init; any(bvec_from_comparison); update))
├── ctrl-do-while.glsl       (do { } while (any(bvec_from_comparison)))
├── ctrl-ternary.glsl        (any(bvec_from_comparison) ? expr1 : expr2)
├── edge-zero.glsl           (ivec2(0, 0) patterns)
├── edge-min-max.glsl        (min/max component values)
├── edge-overflow.glsl       (overflow in component-wise operations)
└── edge-mixed-components.glsl (mixed positive/negative patterns)
```

## Test File Patterns

Each test file should follow the pattern from `vec4/` and `bool/` tests:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

ivec2 test_ivec_operation_name() {
    // Test implementation
    return result;
    // Should be ivec2(expected_x, expected_y)
}

// run: test_ivec_operation_name() == ivec2(expected_x, expected_y)
```

## Key Test Categories

### 1. Arithmetic Operators

**op-add.glsl**: Test `+` operator (component-wise)

- `ivec2 + ivec2` → `ivec2` (component-wise addition)
- Test with positive/negative numbers
- Test overflow behavior (implementation-defined)

**op-subtract.glsl**: Test `-` operator (component-wise)

- `ivec2 - ivec2` → `ivec2` (component-wise subtraction)
- Test with positive/negative numbers

**op-multiply.glsl**: Test `*` operator (component-wise)

- `ivec2 * ivec2` → `ivec2` (component-wise multiplication)
- Test with positive/negative numbers

**op-divide.glsl**: Test `/` operator (component-wise)

- `ivec2 / ivec2` → `ivec2` (component-wise division, truncates toward zero)
- Test with positive/negative numbers
- Test division by zero (undefined behavior)

**op-modulo.glsl**: Test `%` operator (component-wise)

- `ivec2 % ivec2` → `ivec2` (component-wise modulo)
- Test with positive/negative numbers
- Test modulo by zero (undefined behavior)

### 2. Comparison Operators

**op-equal.glsl**: Test `==` operator (aggregate comparison)

- `ivec2 == ivec2` → `bool` (true if all components equal)
- Test with matching vectors, partially matching, completely different

**op-not-equal.glsl**: Test `!=` operator (aggregate comparison)

- `ivec2 != ivec2` → `bool` (true if any component differs)

### 3. Unary Operators

**op-unary-minus.glsl**: Test `-` unary operator (component-wise)

- `-ivec2` → `ivec2` (component-wise negation)
- Test with positive/negative values

### 4. Increment/Decrement Operators

**op-increment-pre.glsl**: Test `++` pre-increment

- `++ivec2` → `ivec2` (increment all components, return new value)
- Must be on lvalue

**op-increment-post.glsl**: Test `++` post-increment

- `ivec2++` → `ivec2` (return old value, then increment all components)
- Must be on lvalue

**op-decrement-pre.glsl**: Test `--` pre-decrement

- `--ivec2` → `ivec2` (decrement all components, return new value)
- Must be on lvalue

**op-decrement-post.glsl**: Test `--` post-decrement

- `ivec2--` → `ivec2` (return old value, then decrement all components)
- Must be on lvalue

### 5. Built-in Functions

**fn-less-than.glsl**: Test `lessThan()` built-in

- `lessThan(ivec2, ivec2)` → `bvec2` (component-wise comparison)
- Returns `bvec2(true, false)` if first component of left < first component of right, etc.

**fn-greater-than.glsl**: Test `greaterThan()` built-in

- `greaterThan(ivec2, ivec2)` → `bvec2` (component-wise comparison)

**fn-less-equal.glsl**: Test `lessThanEqual()` built-in

- `lessThanEqual(ivec2, ivec2)` → `bvec2` (component-wise comparison)

**fn-greater-equal.glsl**: Test `greaterThanEqual()` built-in

- `greaterThanEqual(ivec2, ivec2)` → `bvec2` (component-wise comparison)

**fn-equal.glsl**: Test `equal()` built-in

- `equal(ivec2, ivec2)` → `bvec2` (component-wise equality)

**fn-not-equal.glsl**: Test `notEqual()` built-in

- `notEqual(ivec2, ivec2)` → `bvec2` (component-wise inequality)

**fn-min.glsl**: Test `min()` built-in

- `min(ivec2, ivec2)` → `ivec2` (component-wise minimum)

**fn-max.glsl**: Test `max()` built-in

- `max(ivec2, ivec2)` → `ivec2` (component-wise maximum)

**fn-clamp.glsl**: Test `clamp()` built-in

- `clamp(ivec2, ivec2, ivec2)` → `ivec2` (component-wise clamp)

**fn-abs.glsl**: Test `abs()` built-in

- `abs(ivec2)` → `ivec2` (component-wise absolute value)

### 6. Constructors

**from-scalar.glsl**: Test scalar broadcast constructor

- `ivec2(int)` - broadcast single int to both components
- `ivec2(5)` → `ivec2(5, 5)`

**from-scalars.glsl**: Test constructors from multiple scalars

- `ivec2(int, int)` - from 2 ints
- Various combinations of values

**from-shortening.glsl**: Test shortening constructors

- `ivec2(ivec3)` - extract first two components
- `ivec2(ivec4)` - extract first two components
- Verify components are preserved in order

**from-ivec.glsl**: Test identity constructor

- `ivec2(ivec2)` - identity constructor
- Should preserve all components

**from-mixed.glsl**: Test constructors with type conversions

- `ivec2(bool, bool)` - converts to int (false → 0, true → 1)
- `ivec2(float, float)` - truncates fractional part
- Test various numeric inputs

**from-vectors.glsl**: Test identity constructor (alias for from-ivec.glsl)

### 7. Conversions

**to-int.glsl**: Test conversion to scalar int

- `int(ivec2)` - extracts first component
- `int(ivec2(5, 10))` → `5` (first component)

**to-float.glsl**: Test conversion to scalar float

- `float(ivec2)` - converts first component to float

**to-bool.glsl**: Test conversion to scalar bool

- `bool(ivec2)` - converts first component (0 → false, non-zero → true)

**to-uvec.glsl**: Test conversion to uvec2

- `uvec2(ivec2)` - component-wise conversion
- Negative values undefined behavior

**to-vec.glsl**: Test conversion to vec2

- `vec2(ivec2)` - component-wise conversion

### 8. Assignment

**assign-simple.glsl**: Test simple assignment

- `ivec2 a = ivec2(...); ivec2 b = a;` - assignment
- Verify independence (modifying one doesn't affect the other)
- Self-assignment: `ivec2 = ivec2`

**assign-element.glsl**: Test single component assignment

- `ivec2.x = int` - assign to single component by name
- `ivec2[0] = int` - assign to single component by index
- Verify other components unchanged
- Test all components: x, y and indices 0, 1

**assign-swizzle.glsl**: Test multi-component swizzle assignment

- `ivec2.xy = ivec2(...)` - assign to swizzle
- Verify components are updated correctly
- Test various swizzle patterns (no duplicates allowed in assignment)

### 9. Component Access

**access-array.glsl**: Test array-style indexing

- `ivec2[0]`, `ivec2[1]` - array indexing
- Variable indexing: `ivec2[i]` where `i` is computed
- Verify correct component access

**access-component.glsl**: Test component name access

- `ivec2.x`, `ivec2.y` - component access
- Verify correct component values

**access-swizzle.glsl**: Test component swizzling

- `ivec2.xy` → `ivec2` (identity)
- `ivec2.yx` → `ivec2` (reverse)
- `ivec2.xx` → `ivec2` (duplicate)
- `ivec2.yy` → `ivec2` (duplicate)
- Test all name sets: `xy`, `rg`, `st`
- Test various patterns: `xy`, `yx`, `xx`, `yy`, etc.

### 10. Control Flow

**ctrl-if.glsl**: Test `if` statements with ivec2

- Control flow conditions must be scalar `bool`, so use built-ins that return `bvec2`, then `any()` or `all()`
- `if (any(lessThan(ivec2, ivec2)))` - condition using any()
- `if (all(equal(ivec2, ivec2)))` - condition using all()

**ctrl-while.glsl**: Test `while` loops with ivec2

- `while (any(greaterThan(ivec2, ivec2)))` - loop condition

**ctrl-for.glsl**: Test `for` loops with ivec2

- `for (init; any(notEqual(ivec2, ivec2)); update)` - for loop condition

**ctrl-do-while.glsl**: Test `do-while` loops with ivec2

- `do { } while (any(lessThan(ivec2, ivec2)))` - do-while condition

**ctrl-ternary.glsl**: Test ternary operator with ivec2

- `any(equal(ivec2, ivec2)) ? expr1 : expr2` - ternary with ivec2 condition

### 11. Edge Cases

**edge-zero.glsl**: Test edge case with all components zero

- `ivec2(0, 0)` patterns
- Operations with zero vectors

**edge-min-max.glsl**: Test min/max component values

- Components with INT_MIN, INT_MAX values
- Operations near limits

**edge-overflow.glsl**: Test overflow in component-wise operations

- Addition/multiplication that exceeds INT_MAX per component
- Implementation-defined behavior

**edge-mixed-components.glsl**: Test various mixed positive/negative patterns

- Different patterns: `(1, -1)`, `(-5, 10)`, etc.
- Verify component-wise operations work correctly

## Implementation Notes

1. **Test Format**: Follow the exact format from `vec4/` and `bool/` tests with:

   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:

   - All operators from GLSL spec (operators.adoc)
   - All constructor forms (operators.adoc, constructors section)
   - All conversion forms (operators.adoc, conversion section)
   - All built-in functions (builtinfunctions.adoc: GenIType functions)
   - Component access (swizzling, indexing)
   - Control flow requirements (statements.adoc: conditions must be bool, use any/all for bvec)

3. **Key Differences from bvec2**:

   - Arithmetic operations instead of logical operations
   - Comparison operators return `bool` (aggregate) instead of `bvec2` (component-wise)
   - Use `equal()`/`notEqual()` built-ins for component-wise comparison returning `bvec2`
   - Modulus operator (`%`) available
   - Increment/decrement operators work on vectors
   - Type conversions to/from other vector types

4. **Expected Failures**: These tests are expected to fail initially, especially:

   - Built-in functions (`lessThan()`, `equal()`, `min()`, `max()`, etc.)
   - Some constructor forms (shortening constructors, mixed types)
   - Some conversion forms
   - Swizzle assignment
   - Increment/decrement operators

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/relational/equal.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/relational/not.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/assignment/simple-assignment.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/indexing/swizzling.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/assignment/element-assignment.glsl`

## Files to Create

Create 32 test files in the flat `ivec2/` directory structure above, with each file containing 3-10 test functions following the vec4/bool pattern. All files use the prefix naming convention:

- `op-*` for operators (arithmetic, comparison, unary, increment/decrement)
- `fn-*` for built-in functions (comparison, min/max, abs)
- `from-*` for constructors (from-scalar, from-scalars, from-shortening, from-ivec, from-mixed, from-vectors)
- `to-*` for conversions
- `assign-*` for assignments (assign-simple, assign-element, assign-swizzle)
- `access-*` for component access (array, component, swizzle)
- `ctrl-*` for control flow
- `edge-*` for edge cases

## GLSL Spec References

- **operators.adoc**: Constructors (lines 171-229), Vector components (lines 427-579), Arithmetic operators (lines 580-700), Conversions (lines 908-1100)
- **builtinfunctions.adoc**: Relational functions (lines 1228-1312), Common functions for `GenIType`
