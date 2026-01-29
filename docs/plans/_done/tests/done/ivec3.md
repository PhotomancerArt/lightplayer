# Plan: Create Comprehensive ivec3 Tests

## Overview

Create a complete test suite for integer vector type `ivec3` in `lightplayer/crates/lp-glsl-filetests/filetests/ivec3/` following the organizational pattern used in `vec4/` and `bool/`. These tests will comprehensively cover the GLSL signed integer vector specification for `ivec3` and are expected to fail initially, serving as a specification for implementing the rest of the compiler.

## Directory Structure

Following the flat naming convention with prefixes (like `vec4/` and `bool/`), create tests in a single `ivec3/` directory:

```javascript
ivec3/
├── op-add.glsl              (ivec3 + ivec3 -> ivec3, component-wise)
├── op-subtract.glsl         (ivec3 - ivec3 -> ivec3, component-wise)
├── op-multiply.glsl         (ivec3 * ivec3 -> ivec3, component-wise)
├── op-divide.glsl           (ivec3 / ivec3 -> ivec3, component-wise, truncates)
├── op-modulo.glsl           (ivec3 % ivec3 -> ivec3, component-wise)
├── op-equal.glsl            (ivec3 == ivec3 -> bool, aggregate comparison)
├── op-not-equal.glsl        (ivec3 != ivec3 -> bool, aggregate comparison)
├── op-unary-minus.glsl      (-ivec3 -> ivec3, component-wise negation)
├── op-increment-pre.glsl    (++ivec3 -> ivec3, pre-increment)
├── op-increment-post.glsl   (ivec3++ -> ivec3, post-increment)
├── op-decrement-pre.glsl    (--ivec3 -> ivec3, pre-decrement)
├── op-decrement-post.glsl   (ivec3-- -> ivec3, post-decrement)
├── fn-less-than.glsl        (lessThan(ivec3, ivec3) -> bvec3)
├── fn-greater-than.glsl     (greaterThan(ivec3, ivec3) -> bvec3)
├── fn-less-equal.glsl       (lessThanEqual(ivec3, ivec3) -> bvec3)
├── fn-greater-equal.glsl    (greaterThanEqual(ivec3, ivec3) -> bvec3)
├── fn-equal.glsl            (equal(ivec3, ivec3) -> bvec3, component-wise)
├── fn-not-equal.glsl        (notEqual(ivec3, ivec3) -> bvec3, component-wise)
├── fn-min.glsl              (min(ivec3, ivec3) -> ivec3, component-wise)
├── fn-max.glsl              (max(ivec3, ivec3) -> ivec3, component-wise)
├── fn-clamp.glsl            (clamp(ivec3, ivec3, ivec3) -> ivec3, component-wise)
├── fn-abs.glsl              (abs(ivec3) -> ivec3, component-wise)
├── from-scalar.glsl         (ivec3(int) - broadcast)
├── from-scalars.glsl        (ivec3(int, int, int))
├── from-vectors.glsl        (ivec3(ivec2, int), ivec3(int, ivec2) - vector combinations)
├── from-shortening.glsl     (ivec3(ivec4) - shortening)
├── from-ivec.glsl           (ivec3(ivec3) - identity)
├── from-mixed.glsl          (ivec3(bool, bool, bool), ivec3(float, float, float) - conversions)
├── to-int.glsl              (int(ivec3) - extract first component)
├── to-float.glsl            (float(ivec3) - extract first component)
├── to-bool.glsl             (bool(ivec3) - extract first component)
├── to-uvec.glsl             (uvec3(ivec3) - component-wise conversion)
├── to-vec.glsl              (vec3(ivec3) - component-wise conversion)
├── assign-simple.glsl       (ivec3 = ivec3)
├── assign-element.glsl      (ivec3.x = int, ivec3[0] = int - single component)
├── assign-swizzle.glsl      (ivec3.xy = ivec2, ivec3.xyz = ivec3 - multi-component swizzle)
├── access-array.glsl        (ivec3[0], ivec3[1], ivec3[2])
├── access-component.glsl    (ivec3.x, ivec3.y, ivec3.z)
├── access-swizzle.glsl      (ivec3.xy, ivec3.xyz, ivec3.zyx, etc.)
├── ctrl-if.glsl             (if (any(bvec_from_comparison)) - control flow)
├── ctrl-while.glsl          (while (any(bvec_from_comparison)))
├── ctrl-for.glsl            (for (init; any(bvec_from_comparison); update))
├── ctrl-do-while.glsl       (do { } while (any(bvec_from_comparison)))
├── ctrl-ternary.glsl        (any(bvec_from_comparison) ? expr1 : expr2)
├── edge-zero.glsl           (ivec3(0, 0, 0) patterns)
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

ivec3 test_ivec_operation_name() {
    // Test implementation
    return result;
    // Should be ivec3(expected_x, expected_y, expected_z)
}

// run: test_ivec_operation_name() == ivec3(expected_x, expected_y, expected_z)
```

## Key Test Categories

### 1. Arithmetic Operators

**op-add.glsl**: Test `+` operator (component-wise)

- `ivec3 + ivec3` → `ivec3` (component-wise addition)
- Test with positive/negative numbers
- Test overflow behavior (implementation-defined)

**op-subtract.glsl**: Test `-` operator (component-wise)

- `ivec3 - ivec3` → `ivec3` (component-wise subtraction)
- Test with positive/negative numbers

**op-multiply.glsl**: Test `*` operator (component-wise)

- `ivec3 * ivec3` → `ivec3` (component-wise multiplication)
- Test with positive/negative numbers

**op-divide.glsl**: Test `/` operator (component-wise)

- `ivec3 / ivec3` → `ivec3` (component-wise division, truncates toward zero)
- Test with positive/negative numbers
- Test division by zero (undefined behavior)

**op-modulo.glsl**: Test `%` operator (component-wise)

- `ivec3 % ivec3` → `ivec3` (component-wise modulo)
- Test with positive/negative numbers
- Test modulo by zero (undefined behavior)

### 2. Comparison Operators

**op-equal.glsl**: Test `==` operator (aggregate comparison)

- `ivec3 == ivec3` → `bool` (true if all components equal)
- Test with matching vectors, partially matching, completely different

**op-not-equal.glsl**: Test `!=` operator (aggregate comparison)

- `ivec3 != ivec3` → `bool` (true if any component differs)

### 3. Unary Operators

**op-unary-minus.glsl**: Test `-` unary operator (component-wise)

- `-ivec3` → `ivec3` (component-wise negation)
- Test with positive/negative values

### 4. Increment/Decrement Operators

**op-increment-pre.glsl**: Test `++` pre-increment

- `++ivec3` → `ivec3` (increment all components, return new value)
- Must be on lvalue

**op-increment-post.glsl**: Test `++` post-increment

- `ivec3++` → `ivec3` (return old value, then increment all components)
- Must be on lvalue

**op-decrement-pre.glsl**: Test `--` pre-decrement

- `--ivec3` → `ivec3` (decrement all components, return new value)
- Must be on lvalue

**op-decrement-post.glsl**: Test `--` post-decrement

- `ivec3--` → `ivec3` (return old value, then decrement all components)
- Must be on lvalue

### 5. Built-in Functions

**fn-less-than.glsl**: Test `lessThan()` built-in

- `lessThan(ivec3, ivec3)` → `bvec3` (component-wise comparison)

**fn-greater-than.glsl**: Test `greaterThan()` built-in

- `greaterThan(ivec3, ivec3)` → `bvec3` (component-wise comparison)

**fn-less-equal.glsl**: Test `lessThanEqual()` built-in

- `lessThanEqual(ivec3, ivec3)` → `bvec3` (component-wise comparison)

**fn-greater-equal.glsl**: Test `greaterThanEqual()` built-in

- `greaterThanEqual(ivec3, ivec3)` → `bvec3` (component-wise comparison)

**fn-equal.glsl**: Test `equal()` built-in

- `equal(ivec3, ivec3)` → `bvec3` (component-wise equality)

**fn-not-equal.glsl**: Test `notEqual()` built-in

- `notEqual(ivec3, ivec3)` → `bvec3` (component-wise inequality)

**fn-min.glsl**: Test `min()` built-in

- `min(ivec3, ivec3)` → `ivec3` (component-wise minimum)

**fn-max.glsl**: Test `max()` built-in

- `max(ivec3, ivec3)` → `ivec3` (component-wise maximum)

**fn-clamp.glsl**: Test `clamp()` built-in

- `clamp(ivec3, ivec3, ivec3)` → `ivec3` (component-wise clamp)

**fn-abs.glsl**: Test `abs()` built-in

- `abs(ivec3)` → `ivec3` (component-wise absolute value)

### 6. Constructors

**from-scalar.glsl**: Test scalar broadcast constructor

- `ivec3(int)` - broadcast single int to all components
- `ivec3(5)` → `ivec3(5, 5, 5)`

**from-scalars.glsl**: Test constructors from multiple scalars

- `ivec3(int, int, int)` - from 3 ints
- Various combinations of values

**from-vectors.glsl**: Test constructors from vector combinations

- `ivec3(ivec2, int)` - combine ivec2 and int
- `ivec3(int, ivec2)` - combine int and ivec2
- Test all valid combinations

**from-shortening.glsl**: Test shortening constructor

- `ivec3(ivec4)` - extract first three components
- Verify components are preserved in order

**from-ivec.glsl**: Test identity constructor

- `ivec3(ivec3)` - identity constructor
- Should preserve all components

**from-mixed.glsl**: Test constructors with type conversions

- `ivec3(bool, bool, bool)` - converts to int (false → 0, true → 1)
- `ivec3(float, float, float)` - truncates fractional part
- Test various numeric inputs

### 7. Conversions

**to-int.glsl**: Test conversion to scalar int

- `int(ivec3)` - extracts first component
- `int(ivec3(5, 10, 15))` → `5` (first component)

**to-float.glsl**: Test conversion to scalar float

- `float(ivec3)` - converts first component to float

**to-bool.glsl**: Test conversion to scalar bool

- `bool(ivec3)` - converts first component (0 → false, non-zero → true)

**to-uvec.glsl**: Test conversion to uvec3

- `uvec3(ivec3)` - component-wise conversion
- Negative values undefined behavior

**to-vec.glsl**: Test conversion to vec3

- `vec3(ivec3)` - component-wise conversion

### 8. Assignment

**assign-simple.glsl**: Test simple assignment

- `ivec3 a = ivec3(...); ivec3 b = a;` - assignment
- Verify independence (modifying one doesn't affect the other)
- Self-assignment: `ivec3 = ivec3`

**assign-element.glsl**: Test single component assignment

- `ivec3.x = int` - assign to single component by name
- `ivec3[0] = int` - assign to single component by index
- Verify other components unchanged
- Test all components: x, y, z and indices 0, 1, 2

**assign-swizzle.glsl**: Test multi-component swizzle assignment

- `ivec3.xy = ivec2(...)` - assign to swizzle
- `ivec3.xyz = ivec3(...)` - assign to swizzle
- Verify components are updated correctly
- Test various swizzle patterns (no duplicates allowed in assignment)

### 9. Component Access

**access-array.glsl**: Test array-style indexing

- `ivec3[0]`, `ivec3[1]`, `ivec3[2]` - array indexing
- Variable indexing: `ivec3[i]` where `i` is computed
- Verify correct component access

**access-component.glsl**: Test component name access

- `ivec3.x`, `ivec3.y`, `ivec3.z` - component access
- Verify correct component values

**access-swizzle.glsl**: Test component swizzling

- `ivec3.xy` → `ivec2`
- `ivec3.xyz` → `ivec3` (identity)
- `ivec3.zyx` → `ivec3` (reverse)
- `ivec3.xxy` → `ivec3` (duplicate)
- Test all name sets: `xyz`, `rgb`, `stp`
- Test various patterns: `xy`, `xz`, `yz`, `zyx`, etc.

### 10. Control Flow

**ctrl-if.glsl**: Test `if` statements with ivec3

- Control flow conditions must be scalar `bool`, so use built-ins that return `bvec3`, then `any()` or `all()`
- `if (any(lessThan(ivec3, ivec3)))` - condition using any()
- `if (all(equal(ivec3, ivec3)))` - condition using all()

**ctrl-while.glsl**: Test `while` loops with ivec3

- `while (any(greaterThan(ivec3, ivec3)))` - loop condition

**ctrl-for.glsl**: Test `for` loops with ivec3

- `for (init; any(notEqual(ivec3, ivec3)); update)` - for loop condition

**ctrl-do-while.glsl**: Test `do-while` loops with ivec3

- `do { } while (any(lessThan(ivec3, ivec3)))` - do-while condition

**ctrl-ternary.glsl**: Test ternary operator with ivec3

- `any(equal(ivec3, ivec3)) ? expr1 : expr2` - ternary with ivec3 condition

### 11. Edge Cases

**edge-zero.glsl**: Test edge case with all components zero

- `ivec3(0, 0, 0)` patterns
- Operations with zero vectors

**edge-min-max.glsl**: Test min/max component values

- Components with INT_MIN, INT_MAX values
- Operations near limits

**edge-overflow.glsl**: Test overflow in component-wise operations

- Addition/multiplication that exceeds INT_MAX per component
- Implementation-defined behavior

**edge-mixed-components.glsl**: Test various mixed positive/negative patterns

- Different patterns: `(1, -1, 5)`, `(-5, 10, -3)`, etc.
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

3. **Key Differences from ivec2**:

   - Additional component (z) and index [2]
   - Vector combination constructors: `ivec3(ivec2, int)`, `ivec3(int, ivec2)`
   - Extended swizzle patterns including z component
   - More complex shortening constructor (from ivec4 only, not from ivec3)

4. **Expected Failures**: These tests are expected to fail initially, especially:

   - Built-in functions (`lessThan()`, `equal()`, `min()`, `max()`, etc.)
   - Some constructor forms (shortening constructors, mixed types, vector combinations)
   - Some conversion forms
   - Swizzle assignment
   - Increment/decrement operators

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/relational/equal.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/relational/not.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/assignment/simple-assignment.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/indexing/swizzling.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/assignment/element-assignment.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/constructors/shortening.glsl`

## Files to Create

Create 33 test files in the flat `ivec3/` directory structure above, with each file containing 3-10 test functions following the vec4/bool pattern. All files use the prefix naming convention:

- `op-*` for operators (arithmetic, comparison, unary, increment/decrement)
- `fn-*` for built-in functions (comparison, min/max, abs)
- `from-*` for constructors (from-scalar, from-scalars, from-vectors, from-shortening, from-ivec, from-mixed)
- `to-*` for conversions
- `assign-*` for assignments (assign-simple, assign-element, assign-swizzle)
- `access-*` for component access (array, component, swizzle)
- `ctrl-*` for control flow
- `edge-*` for edge cases

## GLSL Spec References

- **operators.adoc**: Constructors (lines 171-229), Vector components (lines 427-579), Arithmetic operators (lines 580-700), Conversions (lines 908-1100)
- **builtinfunctions.adoc**: Relational functions (lines 1228-1312), Common functions for `GenIType`
