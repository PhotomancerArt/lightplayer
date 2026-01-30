# Plan: Create Comprehensive ivec4 Tests

## Overview

Create a complete test suite for integer vector type `ivec4` in `lightplayer/crates/lp-glsl-filetests/filetests/ivec4/` following the organizational pattern used in `vec4/` and `bool/`. These tests will comprehensively cover the GLSL signed integer vector specification for `ivec4` and are expected to fail initially, serving as a specification for implementing the rest of the compiler.

## Directory Structure

Following the flat naming convention with prefixes (like `vec4/` and `bool/`), create tests in a single `ivec4/` directory:

```javascript
ivec4/
├── op-add.glsl              (ivec4 + ivec4 -> ivec4, component-wise)
├── op-subtract.glsl         (ivec4 - ivec4 -> ivec4, component-wise)
├── op-multiply.glsl         (ivec4 * ivec4 -> ivec4, component-wise)
├── op-divide.glsl           (ivec4 / ivec4 -> ivec4, component-wise, truncates)
├── op-modulo.glsl           (ivec4 % ivec4 -> ivec4, component-wise)
├── op-equal.glsl            (ivec4 == ivec4 -> bool, aggregate comparison)
├── op-not-equal.glsl        (ivec4 != ivec4 -> bool, aggregate comparison)
├── op-unary-minus.glsl      (-ivec4 -> ivec4, component-wise negation)
├── op-increment-pre.glsl    (++ivec4 -> ivec4, pre-increment)
├── op-increment-post.glsl   (ivec4++ -> ivec4, post-increment)
├── op-decrement-pre.glsl    (--ivec4 -> ivec4, pre-decrement)
├── op-decrement-post.glsl   (ivec4-- -> ivec4, post-decrement)
├── fn-less-than.glsl        (lessThan(ivec4, ivec4) -> bvec4)
├── fn-greater-than.glsl     (greaterThan(ivec4, ivec4) -> bvec4)
├── fn-less-equal.glsl       (lessThanEqual(ivec4, ivec4) -> bvec4)
├── fn-greater-equal.glsl    (greaterThanEqual(ivec4, ivec4) -> bvec4)
├── fn-equal.glsl            (equal(ivec4, ivec4) -> bvec4, component-wise)
├── fn-not-equal.glsl        (notEqual(ivec4, ivec4) -> bvec4, component-wise)
├── fn-min.glsl              (min(ivec4, ivec4) -> ivec4, component-wise)
├── fn-max.glsl              (max(ivec4, ivec4) -> ivec4, component-wise)
├── fn-clamp.glsl            (clamp(ivec4, ivec4, ivec4) -> ivec4, component-wise)
├── fn-abs.glsl              (abs(ivec4) -> ivec4, component-wise)
├── from-scalar.glsl         (ivec4(int) - broadcast)
├── from-scalars.glsl        (ivec4(int, int, int, int))
├── from-vectors.glsl        (ivec4(ivec2, ivec2), ivec4(ivec3, int), etc. - vector combinations)
├── from-ivec.glsl           (ivec4(ivec4) - identity)
├── from-mixed.glsl          (ivec4(bool, bool, bool, bool), ivec4(float, float, float, float) - conversions)
├── to-int.glsl              (int(ivec4) - extract first component)
├── to-float.glsl            (float(ivec4) - extract first component)
├── to-bool.glsl             (bool(ivec4) - extract first component)
├── to-uvec.glsl             (uvec4(ivec4) - component-wise conversion)
├── to-vec.glsl              (vec4(ivec4) - component-wise conversion)
├── assign-simple.glsl       (ivec4 = ivec4)
├── assign-element.glsl      (ivec4.x = int, ivec4[0] = int - single component)
├── assign-swizzle.glsl      (ivec4.xy = ivec2, ivec4.xyz = ivec3 - multi-component swizzle)
├── access-array.glsl        (ivec4[0], ivec4[1], ivec4[2], ivec4[3])
├── access-component.glsl    (ivec4.x, ivec4.y, ivec4.z, ivec4.w)
├── access-swizzle.glsl      (ivec4.xy, ivec4.xyz, ivec4.xyzw, ivec4.wzyx, etc.)
├── ctrl-if.glsl             (if (any(bvec_from_comparison)) - control flow)
├── ctrl-while.glsl          (while (any(bvec_from_comparison)))
├── ctrl-for.glsl            (for (init; any(bvec_from_comparison); update))
├── ctrl-do-while.glsl       (do { } while (any(bvec_from_comparison)))
├── ctrl-ternary.glsl        (any(bvec_from_comparison) ? expr1 : expr2)
├── edge-zero.glsl           (ivec4(0, 0, 0, 0) patterns)
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

ivec4 test_ivec_operation_name() {
    // Test implementation
    return result;
    // Should be ivec4(expected_x, expected_y, expected_z, expected_w)
}

// run: test_ivec_operation_name() == ivec4(expected_x, expected_y, expected_z, expected_w)
```

## Key Test Categories

### 1. Arithmetic Operators

**op-add.glsl**: Test `+` operator (component-wise)

- `ivec4 + ivec4` → `ivec4` (component-wise addition)
- Test with positive/negative numbers
- Test overflow behavior (implementation-defined)

**op-subtract.glsl**: Test `-` operator (component-wise)

- `ivec4 - ivec4` → `ivec4` (component-wise subtraction)
- Test with positive/negative numbers

**op-multiply.glsl**: Test `*` operator (component-wise)

- `ivec4 * ivec4` → `ivec4` (component-wise multiplication)
- Test with positive/negative numbers

**op-divide.glsl**: Test `/` operator (component-wise)

- `ivec4 / ivec4` → `ivec4` (component-wise division, truncates toward zero)
- Test with positive/negative numbers
- Test division by zero (undefined behavior)

**op-modulo.glsl**: Test `%` operator (component-wise)

- `ivec4 % ivec4` → `ivec4` (component-wise modulo)
- Test with positive/negative numbers
- Test modulo by zero (undefined behavior)

### 2. Comparison Operators

**op-equal.glsl**: Test `==` operator (aggregate comparison)

- `ivec4 == ivec4` → `bool` (true if all components equal)
- Test with matching vectors, partially matching, completely different

**op-not-equal.glsl**: Test `!=` operator (aggregate comparison)

- `ivec4 != ivec4` → `bool` (true if any component differs)

### 3. Unary Operators

**op-unary-minus.glsl**: Test `-` unary operator (component-wise)

- `-ivec4` → `ivec4` (component-wise negation)
- Test with positive/negative values

### 4. Increment/Decrement Operators

**op-increment-pre.glsl**: Test `++` pre-increment

- `++ivec4` → `ivec4` (increment all components, return new value)
- Must be on lvalue

**op-increment-post.glsl**: Test `++` post-increment

- `ivec4++` → `ivec4` (return old value, then increment all components)
- Must be on lvalue

**op-decrement-pre.glsl**: Test `--` pre-decrement

- `--ivec4` → `ivec4` (decrement all components, return new value)
- Must be on lvalue

**op-decrement-post.glsl**: Test `--` post-decrement

- `ivec4--` → `ivec4` (return old value, then decrement all components)
- Must be on lvalue

### 5. Built-in Functions

**fn-less-than.glsl**: Test `lessThan()` built-in

- `lessThan(ivec4, ivec4)` → `bvec4` (component-wise comparison)

**fn-greater-than.glsl**: Test `greaterThan()` built-in

- `greaterThan(ivec4, ivec4)` → `bvec4` (component-wise comparison)

**fn-less-equal.glsl**: Test `lessThanEqual()` built-in

- `lessThanEqual(ivec4, ivec4)` → `bvec4` (component-wise comparison)

**fn-greater-equal.glsl**: Test `greaterThanEqual()` built-in

- `greaterThanEqual(ivec4, ivec4)` → `bvec4` (component-wise comparison)

**fn-equal.glsl**: Test `equal()` built-in

- `equal(ivec4, ivec4)` → `bvec4` (component-wise equality)

**fn-not-equal.glsl**: Test `notEqual()` built-in

- `notEqual(ivec4, ivec4)` → `bvec4` (component-wise inequality)

**fn-min.glsl**: Test `min()` built-in

- `min(ivec4, ivec4)` → `ivec4` (component-wise minimum)

**fn-max.glsl**: Test `max()` built-in

- `max(ivec4, ivec4)` → `ivec4` (component-wise maximum)

**fn-clamp.glsl**: Test `clamp()` built-in

- `clamp(ivec4, ivec4, ivec4)` → `ivec4` (component-wise clamp)

**fn-abs.glsl**: Test `abs()` built-in

- `abs(ivec4)` → `ivec4` (component-wise absolute value)

### 6. Constructors

**from-scalar.glsl**: Test scalar broadcast constructor

- `ivec4(int)` - broadcast single int to all components
- `ivec4(5)` → `ivec4(5, 5, 5, 5)`

**from-scalars.glsl**: Test constructors from multiple scalars

- `ivec4(int, int, int, int)` - from 4 ints
- Various combinations of values

**from-vectors.glsl**: Test constructors from vector combinations

- `ivec4(ivec2, ivec2)` - combine two ivec2s
- `ivec4(ivec3, int)` - combine ivec3 and int
- `ivec4(int, ivec3)` - combine int and ivec3
- `ivec4(ivec2, int, int)` - various combinations
- Test all valid combinations for constructing ivec4

**from-ivec.glsl**: Test identity constructor

- `ivec4(ivec4)` - identity constructor
- Should preserve all components

**from-mixed.glsl**: Test constructors with type conversions

- `ivec4(bool, bool, bool, bool)` - converts to int (false → 0, true → 1)
- `ivec4(float, float, float, float)` - truncates fractional part
- Test various numeric inputs

### 7. Conversions

**to-int.glsl**: Test conversion to scalar int

- `int(ivec4)` - extracts first component
- `int(ivec4(5, 10, 15, 20))` → `5` (first component)

**to-float.glsl**: Test conversion to scalar float

- `float(ivec4)` - converts first component to float

**to-bool.glsl**: Test conversion to scalar bool

- `bool(ivec4)` - converts first component (0 → false, non-zero → true)

**to-uvec.glsl**: Test conversion to uvec4

- `uvec4(ivec4)` - component-wise conversion
- Negative values undefined behavior

**to-vec.glsl**: Test conversion to vec4

- `vec4(ivec4)` - component-wise conversion

### 8. Assignment

**assign-simple.glsl**: Test simple assignment

- `ivec4 a = ivec4(...); ivec4 b = a;` - assignment
- Verify independence (modifying one doesn't affect the other)
- Self-assignment: `ivec4 = ivec4`

**assign-element.glsl**: Test single component assignment

- `ivec4.x = int` - assign to single component by name
- `ivec4[0] = int` - assign to single component by index
- Verify other components unchanged
- Test all components: x, y, z, w and indices 0, 1, 2, 3

**assign-swizzle.glsl**: Test multi-component swizzle assignment

- `ivec4.xy = ivec2(...)` - assign to swizzle
- `ivec4.xyz = ivec3(...)` - assign to swizzle
- Verify components are updated correctly
- Test various swizzle patterns (no duplicates allowed in assignment)

### 9. Component Access

**access-array.glsl**: Test array-style indexing

- `ivec4[0]`, `ivec4[1]`, `ivec4[2]`, `ivec4[3]` - array indexing
- Variable indexing: `ivec4[i]` where `i` is computed
- Verify correct component access

**access-component.glsl**: Test component name access

- `ivec4.x`, `ivec4.y`, `ivec4.z`, `ivec4.w` - component access
- Verify correct component values

**access-swizzle.glsl**: Test component swizzling

- `ivec4.xy` → `ivec2`
- `ivec4.xyz` → `ivec3`
- `ivec4.xyzw` → `ivec4` (identity)
- `ivec4.wzyx` → `ivec4` (reverse)
- `ivec4.xxyy` → `ivec4` (duplicate)
- Test all name sets: `xyzw`, `rgba`, `stpq`
- Test various patterns: `xy`, `zw`, `xz`, `wzyx`, etc.

### 10. Control Flow

**ctrl-if.glsl**: Test `if` statements with ivec4

- Control flow conditions must be scalar `bool`, so use built-ins that return `bvec4`, then `any()` or `all()`
- `if (any(lessThan(ivec4, ivec4)))` - condition using any()
- `if (all(equal(ivec4, ivec4)))` - condition using all()

**ctrl-while.glsl**: Test `while` loops with ivec4

- `while (any(greaterThan(ivec4, ivec4)))` - loop condition

**ctrl-for.glsl**: Test `for` loops with ivec4

- `for (init; any(notEqual(ivec4, ivec4)); update)` - for loop condition

**ctrl-do-while.glsl**: Test `do-while` loops with ivec4

- `do { } while (any(lessThan(ivec4, ivec4)))` - do-while condition

**ctrl-ternary.glsl**: Test ternary operator with ivec4

- `any(equal(ivec4, ivec4)) ? expr1 : expr2` - ternary with ivec4 condition

### 11. Edge Cases

**edge-zero.glsl**: Test edge case with all components zero

- `ivec4(0, 0, 0, 0)` patterns
- Operations with zero vectors

**edge-min-max.glsl**: Test min/max component values

- Components with INT_MIN, INT_MAX values
- Operations near limits

**edge-overflow.glsl**: Test overflow in component-wise operations

- Addition/multiplication that exceeds INT_MAX per component
- Implementation-defined behavior

**edge-mixed-components.glsl**: Test various mixed positive/negative patterns

- Different patterns: `(1, -1, 5, -3)`, `(-5, 10, -3, 8)`, etc.
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

3. **Key Differences from ivec3**:

   - Additional component (w) and index [3]
   - More vector constructor combinations: `ivec4(ivec2, ivec2)`, `ivec4(ivec3, int)`, etc.
   - Extended swizzle patterns including w component
   - No shortening constructor (ivec4 is the largest integer vector type)

4. **Expected Failures**: These tests are expected to fail initially, especially:

   - Built-in functions (`lessThan()`, `equal()`, `min()`, `max()`, etc.)
   - Some constructor forms (mixed types, vector combinations)
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

Create 33 test files in the flat `ivec4/` directory structure above, with each file containing 3-10 test functions following the vec4/bool pattern. All files use the prefix naming convention:

- `op-*` for operators (arithmetic, comparison, unary, increment/decrement)
- `fn-*` for built-in functions (comparison, min/max, abs)
- `from-*` for constructors (from-scalar, from-scalars, from-vectors, from-ivec, from-mixed)
- `to-*` for conversions
- `assign-*` for assignments (assign-simple, assign-element, assign-swizzle)
- `access-*` for component access (array, component, swizzle)
- `ctrl-*` for control flow
- `edge-*` for edge cases

## GLSL Spec References

- **operators.adoc**: Constructors (lines 171-229), Vector components (lines 427-579), Arithmetic operators (lines 580-700), Conversions (lines 908-1100)
- **builtinfunctions.adoc**: Relational functions (lines 1228-1312), Common functions for `GenIType`
