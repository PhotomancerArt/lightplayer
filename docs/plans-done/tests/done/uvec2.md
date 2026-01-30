# Plan: Create Comprehensive uvec2 Tests

## Overview

Create a complete test suite for unsigned integer vector type `uvec2` in `lightplayer/crates/lp-glsl-filetests/filetests/uvec2/` following the organizational pattern used in `vec4/` and `bool/`. These tests will comprehensively cover the GLSL unsigned integer vector specification for `uvec2` and are expected to fail initially, serving as a specification for implementing the rest of the compiler.

## Directory Structure

Following the flat naming convention with prefixes (like `vec4/` and `bool/`), create tests in a single `uvec2/` directory:

```javascript
uvec2/
├── op-add.glsl              (uvec2 + uvec2 -> uvec2, component-wise)
├── op-subtract.glsl         (uvec2 - uvec2 -> uvec2, component-wise)
├── op-multiply.glsl         (uvec2 * uvec2 -> uvec2, component-wise)
├── op-divide.glsl           (uvec2 / uvec2 -> uvec2, component-wise, truncates)
├── op-modulo.glsl           (uvec2 % uvec2 -> uvec2, component-wise)
├── op-equal.glsl            (uvec2 == uvec2 -> bool, aggregate comparison)
├── op-not-equal.glsl        (uvec2 != uvec2 -> bool, aggregate comparison)
├── op-unary-plus.glsl       (+uvec2 -> uvec2)
├── op-increment-pre.glsl    (++uvec2 -> uvec2, pre-increment)
├── op-increment-post.glsl   (uvec2++ -> uvec2, post-increment)
├── op-decrement-pre.glsl    (--uvec2 -> uvec2, pre-decrement)
├── op-decrement-post.glsl   (uvec2-- -> uvec2, post-decrement)
├── fn-less-than.glsl        (lessThan(uvec2, uvec2) -> bvec2)
├── fn-greater-than.glsl     (greaterThan(uvec2, uvec2) -> bvec2)
├── fn-less-equal.glsl       (lessThanEqual(uvec2, uvec2) -> bvec2)
├── fn-greater-equal.glsl    (greaterThanEqual(uvec2, uvec2) -> bvec2)
├── fn-equal.glsl            (equal(uvec2, uvec2) -> bvec2, component-wise)
├── fn-not-equal.glsl        (notEqual(uvec2, uvec2) -> bvec2, component-wise)
├── fn-min.glsl              (min(uvec2, uvec2) -> uvec2, component-wise)
├── fn-max.glsl              (max(uvec2, uvec2) -> uvec2, component-wise)
├── fn-clamp.glsl            (clamp(uvec2, uvec2, uvec2) -> uvec2, component-wise)
├── from-scalar.glsl         (uvec2(uint) - broadcast)
├── from-scalars.glsl        (uvec2(uint, uint))
├── from-shortening.glsl     (uvec2(uvec3), uvec2(uvec4) - shortening)
├── from-uvec.glsl           (uvec2(uvec2) - identity)
├── from-mixed.glsl          (uvec2(bool, bool), uvec2(float, float) - conversions)
├── from-vectors.glsl        (uvec2(uvec2) - identity)
├── to-uint.glsl             (uint(uvec2) - extract first component)
├── to-float.glsl            (float(uvec2) - extract first component)
├── to-bool.glsl             (bool(uvec2) - extract first component)
├── to-ivec.glsl             (ivec2(uvec2) - component-wise conversion)
├── to-vec.glsl              (vec2(uvec2) - component-wise conversion)
├── assign-simple.glsl       (uvec2 = uvec2)
├── assign-element.glsl      (uvec2.x = uint, uvec2[0] = uint - single component)
├── assign-swizzle.glsl      (uvec2.xy = uvec2 - multi-component swizzle)
├── access-array.glsl        (uvec2[0], uvec2[1])
├── access-component.glsl    (uvec2.x, uvec2.y)
├── access-swizzle.glsl      (uvec2.xy, uvec2.yx, uvec2.xx, etc.)
├── ctrl-if.glsl             (if (any(bvec_from_comparison)) - control flow)
├── ctrl-while.glsl          (while (any(bvec_from_comparison)))
├── ctrl-for.glsl            (for (init; any(bvec_from_comparison); update))
├── ctrl-do-while.glsl       (do { } while (any(bvec_from_comparison)))
├── ctrl-ternary.glsl        (any(bvec_from_comparison) ? expr1 : expr2)
├── edge-zero.glsl           (uvec2(0u, 0u) patterns)
├── edge-max.glsl            (max uint component values)
├── edge-wraparound.glsl     (wraparound behavior)
└── edge-mixed-components.glsl (various component patterns)
```

## Test File Patterns

Each test file should follow the pattern from `vec4/` and `bool/` tests:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

uvec2 test_uvec_operation_name() {
    // Test implementation
    return result;
    // Should be uvec2(expected_x, expected_y)
}

// run: test_uvec_operation_name() == uvec2(expected_x, expected_y)
```

## Key Test Categories

### 1. Arithmetic Operators

**op-add.glsl**: Test `+` operator (component-wise)
- `uvec2 + uvec2` → `uvec2` (component-wise addition)
- Test with various values
- Test wraparound behavior (modulo 2^32)

**op-subtract.glsl**: Test `-` operator (component-wise)
- `uvec2 - uvec2` → `uvec2` (component-wise subtraction)
- Test with various values
- Test wraparound behavior (modulo 2^32)

**op-multiply.glsl**: Test `*` operator (component-wise)
- `uvec2 * uvec2` → `uvec2` (component-wise multiplication)
- Test with various values
- Test wraparound behavior (modulo 2^32)

**op-divide.glsl**: Test `/` operator (component-wise)
- `uvec2 / uvec2` → `uvec2` (component-wise division, truncates)
- Test with various values
- Test division by zero (undefined behavior)

**op-modulo.glsl**: Test `%` operator (component-wise)
- `uvec2 % uvec2` → `uvec2` (component-wise modulo)
- Test with various values
- Test modulo by zero (undefined behavior)

### 2. Comparison Operators

**op-equal.glsl**: Test `==` operator (aggregate comparison)
- `uvec2 == uvec2` → `bool` (true if all components equal)
- Test with matching vectors, partially matching, completely different

**op-not-equal.glsl**: Test `!=` operator (aggregate comparison)
- `uvec2 != uvec2` → `bool` (true if any component differs)

### 3. Unary Operators

**op-unary-plus.glsl**: Test `+` unary operator
- `+uvec2` → `uvec2` (no-op for uvec2)

### 4. Increment/Decrement Operators

**op-increment-pre.glsl**: Test `++` pre-increment
- `++uvec2` → `uvec2` (increment all components, return new value)
- Must be on lvalue

**op-increment-post.glsl**: Test `++` post-increment
- `uvec2++` → `uvec2` (return old value, then increment all components)
- Must be on lvalue

**op-decrement-pre.glsl**: Test `--` pre-decrement
- `--uvec2` → `uvec2` (decrement all components, return new value)
- Must be on lvalue

**op-decrement-post.glsl**: Test `--` post-decrement
- `uvec2--` → `uvec2` (return old value, then decrement all components)
- Must be on lvalue

### 5. Built-in Functions

**fn-less-than.glsl**: Test `lessThan()` built-in
- `lessThan(uvec2, uvec2)` → `bvec2` (component-wise comparison)

**fn-greater-than.glsl**: Test `greaterThan()` built-in
- `greaterThan(uvec2, uvec2)` → `bvec2` (component-wise comparison)

**fn-less-equal.glsl**: Test `lessThanEqual()` built-in
- `lessThanEqual(uvec2, uvec2)` → `bvec2` (component-wise comparison)

**fn-greater-equal.glsl**: Test `greaterThanEqual()` built-in
- `greaterThanEqual(uvec2, uvec2)` → `bvec2` (component-wise comparison)

**fn-equal.glsl**: Test `equal()` built-in
- `equal(uvec2, uvec2)` → `bvec2` (component-wise equality)

**fn-not-equal.glsl**: Test `notEqual()` built-in
- `notEqual(uvec2, uvec2)` → `bvec2` (component-wise inequality)

**fn-min.glsl**: Test `min()` built-in
- `min(uvec2, uvec2)` → `uvec2` (component-wise minimum)

**fn-max.glsl**: Test `max()` built-in
- `max(uvec2, uvec2)` → `uvec2` (component-wise maximum)

**fn-clamp.glsl**: Test `clamp()` built-in
- `clamp(uvec2, uvec2, uvec2)` → `uvec2` (component-wise clamp)

### 6. Constructors

**from-scalar.glsl**: Test scalar broadcast constructor
- `uvec2(uint)` - broadcast single uint to both components
- `uvec2(5u)` → `uvec2(5u, 5u)`

**from-scalars.glsl**: Test constructors from multiple scalars
- `uvec2(uint, uint)` - from 2 uints
- Various combinations of values

**from-shortening.glsl**: Test shortening constructors
- `uvec2(uvec3)` - extract first two components
- `uvec2(uvec4)` - extract first two components
- Verify components are preserved in order

**from-uvec.glsl**: Test identity constructor
- `uvec2(uvec2)` - identity constructor
- Should preserve all components

**from-mixed.glsl**: Test constructors with type conversions
- `uvec2(bool, bool)` - converts to uint (false → 0u, true → 1u)
- `uvec2(float, float)` - truncates fractional part, must be non-negative
- Test various numeric inputs

**from-vectors.glsl**: Test identity constructor (alias for from-uvec.glsl)

### 7. Conversions

**to-uint.glsl**: Test conversion to scalar uint
- `uint(uvec2)` - extracts first component
- `uint(uvec2(5u, 10u))` → `5u` (first component)

**to-float.glsl**: Test conversion to scalar float
- `float(uvec2)` - converts first component to float

**to-bool.glsl**: Test conversion to scalar bool
- `bool(uvec2)` - converts first component (0u → false, non-zero → true)

**to-ivec.glsl**: Test conversion to ivec2
- `ivec2(uvec2)` - component-wise conversion
- Values may overflow if > INT_MAX

**to-vec.glsl**: Test conversion to vec2
- `vec2(uvec2)` - component-wise conversion

### 8. Assignment

**assign-simple.glsl**: Test simple assignment
- `uvec2 a = uvec2(...); uvec2 b = a;` - assignment
- Verify independence (modifying one doesn't affect the other)
- Self-assignment: `uvec2 = uvec2`

**assign-element.glsl**: Test single component assignment
- `uvec2.x = uint` - assign to single component by name
- `uvec2[0] = uint` - assign to single component by index
- Verify other components unchanged
- Test all components: x, y and indices 0, 1

**assign-swizzle.glsl**: Test multi-component swizzle assignment
- `uvec2.xy = uvec2(...)` - assign to swizzle
- Verify components are updated correctly
- Test various swizzle patterns (no duplicates allowed in assignment)

### 9. Component Access

**access-array.glsl**: Test array-style indexing
- `uvec2[0]`, `uvec2[1]` - array indexing
- Variable indexing: `uvec2[i]` where `i` is computed
- Verify correct component access

**access-component.glsl**: Test component name access
- `uvec2.x`, `uvec2.y` - component access
- Verify correct component values

**access-swizzle.glsl**: Test component swizzling
- `uvec2.xy` → `uvec2` (identity)
- `uvec2.yx` → `uvec2` (reverse)
- `uvec2.xx` → `uvec2` (duplicate)
- `uvec2.yy` → `uvec2` (duplicate)
- Test all name sets: `xy`, `rg`, `st`
- Test various patterns: `xy`, `yx`, `xx`, `yy`, etc.

### 10. Control Flow

**ctrl-if.glsl**: Test `if` statements with uvec2
- Control flow conditions must be scalar `bool`, so use built-ins that return `bvec2`, then `any()` or `all()`
- `if (any(lessThan(uvec2, uvec2)))` - condition using any()
- `if (all(equal(uvec2, uvec2)))` - condition using all()

**ctrl-while.glsl**: Test `while` loops with uvec2
- `while (any(greaterThan(uvec2, uvec2)))` - loop condition

**ctrl-for.glsl**: Test `for` loops with uvec2
- `for (init; any(notEqual(uvec2, uvec2)); update)` - for loop condition

**ctrl-do-while.glsl**: Test `do-while` loops with uvec2
- `do { } while (any(lessThan(uvec2, uvec2)))` - do-while condition

**ctrl-ternary.glsl**: Test ternary operator with uvec2
- `any(equal(uvec2, uvec2)) ? expr1 : expr2` - ternary with uvec2 condition

### 11. Edge Cases

**edge-zero.glsl**: Test edge case with all components zero
- `uvec2(0u, 0u)` patterns
- Operations with zero vectors

**edge-max.glsl**: Test max uint component values
- Components with UINT_MAX (4294967295u)
- Operations at the maximum value
- Wraparound behavior

**edge-wraparound.glsl**: Test wraparound behavior
- Addition that exceeds UINT_MAX (wraps to 0)
- Subtraction that goes below 0 (wraps around)
- Multiplication wraparound

**edge-mixed-components.glsl**: Test various component patterns
- Different patterns: `(1u, 100u)`, `(1000u, 500u)`, etc.
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
   - All built-in functions (builtinfunctions.adoc: unsigned integer functions)
   - Component access (swizzling, indexing)
   - Control flow requirements (statements.adoc: conditions must be bool, use any/all for bvec)

3. **Key Differences from ivec2**:
   - Unsigned arithmetic (no negative values)
   - No unary minus operator
   - Wraparound behavior instead of overflow
   - Different constructor forms (from signed types have restrictions)
   - Different conversion behavior (to signed types may overflow)

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
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/constructors/shortening.glsl`

## Files to Create

Create 32 test files in the flat `uvec2/` directory structure above, with each file containing 3-10 test functions following the vec4/bool pattern. All files use the prefix naming convention:

- `op-*` for operators (arithmetic, comparison, unary, increment/decrement)
- `fn-*` for built-in functions (comparison, min/max)
- `from-*` for constructors (from-scalar, from-scalars, from-shortening, from-uvec, from-mixed, from-vectors)
- `to-*` for conversions
- `assign-*` for assignments (assign-simple, assign-element, assign-swizzle)
- `access-*` for component access (array, component, swizzle)
- `ctrl-*` for control flow
- `edge-*` for edge cases

## GLSL Spec References

- **operators.adoc**: Constructors (lines 171-229), Vector components (lines 427-579), Arithmetic operators (lines 580-700), Conversions (lines 908-1100)
- **builtinfunctions.adoc**: Relational functions (lines 1228-1312), Common functions for unsigned integer types
