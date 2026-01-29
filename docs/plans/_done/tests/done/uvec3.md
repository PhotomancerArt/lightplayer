# Plan: Create Comprehensive uvec3 Tests

## Overview

Create a complete test suite for unsigned integer vector type `uvec3` in `lightplayer/crates/lp-glsl-filetests/filetests/uvec3/` following the organizational pattern used in `vec4/` and `bool/`. These tests will comprehensively cover the GLSL unsigned integer vector specification for `uvec3` and are expected to fail initially, serving as a specification for implementing the rest of the compiler.

## Directory Structure

Following the flat naming convention with prefixes (like `vec4/` and `bool/`), create tests in a single `uvec3/` directory:

```javascript
uvec3/
├── op-add.glsl              (uvec3 + uvec3 -> uvec3, component-wise)
├── op-subtract.glsl         (uvec3 - uvec3 -> uvec3, component-wise)
├── op-multiply.glsl         (uvec3 * uvec3 -> uvec3, component-wise)
├── op-divide.glsl           (uvec3 / uvec3 -> uvec3, component-wise, truncates)
├── op-modulo.glsl           (uvec3 % uvec3 -> uvec3, component-wise)
├── op-equal.glsl            (uvec3 == uvec3 -> bool, aggregate comparison)
├── op-not-equal.glsl        (uvec3 != uvec3 -> bool, aggregate comparison)
├── op-unary-plus.glsl       (+uvec3 -> uvec3)
├── op-increment-pre.glsl    (++uvec3 -> uvec3, pre-increment)
├── op-increment-post.glsl   (uvec3++ -> uvec3, post-increment)
├── op-decrement-pre.glsl    (--uvec3 -> uvec3, pre-decrement)
├── op-decrement-post.glsl   (uvec3-- -> uvec3, post-decrement)
├── fn-less-than.glsl        (lessThan(uvec3, uvec3) -> bvec3)
├── fn-greater-than.glsl     (greaterThan(uvec3, uvec3) -> bvec3)
├── fn-less-equal.glsl       (lessThanEqual(uvec3, uvec3) -> bvec3)
├── fn-greater-equal.glsl    (greaterThanEqual(uvec3, uvec3) -> bvec3)
├── fn-equal.glsl            (equal(uvec3, uvec3) -> bvec3, component-wise)
├── fn-not-equal.glsl        (notEqual(uvec3, uvec3) -> bvec3, component-wise)
├── fn-min.glsl              (min(uvec3, uvec3) -> uvec3, component-wise)
├── fn-max.glsl              (max(uvec3, uvec3) -> uvec3, component-wise)
├── fn-clamp.glsl            (clamp(uvec3, uvec3, uvec3) -> uvec3, component-wise)
├── from-scalar.glsl         (uvec3(uint) - broadcast)
├── from-scalars.glsl        (uvec3(uint, uint, uint))
├── from-vectors.glsl        (uvec3(uvec2, uint), uvec3(uint, uvec2) - vector combinations)
├── from-shortening.glsl     (uvec3(uvec4) - shortening)
├── from-uvec.glsl           (uvec3(uvec3) - identity)
├── from-mixed.glsl          (uvec3(bool, bool, bool), uvec3(float, float, float) - conversions)
├── to-uint.glsl             (uint(uvec3) - extract first component)
├── to-float.glsl            (float(uvec3) - extract first component)
├── to-bool.glsl             (bool(uvec3) - extract first component)
├── to-ivec.glsl             (ivec3(uvec3) - component-wise conversion)
├── to-vec.glsl              (vec3(uvec3) - component-wise conversion)
├── assign-simple.glsl       (uvec3 = uvec3)
├── assign-element.glsl      (uvec3.x = uint, uvec3[0] = uint - single component)
├── assign-swizzle.glsl      (uvec3.xy = uvec2, uvec3.xyz = uvec3 - multi-component swizzle)
├── access-array.glsl        (uvec3[0], uvec3[1], uvec3[2])
├── access-component.glsl    (uvec3.x, uvec3.y, uvec3.z)
├── access-swizzle.glsl      (uvec3.xy, uvec3.xyz, uvec3.zyx, etc.)
├── ctrl-if.glsl             (if (any(bvec_from_comparison)) - control flow)
├── ctrl-while.glsl          (while (any(bvec_from_comparison)))
├── ctrl-for.glsl            (for (init; any(bvec_from_comparison); update))
├── ctrl-do-while.glsl       (do { } while (any(bvec_from_comparison)))
├── ctrl-ternary.glsl        (any(bvec_from_comparison) ? expr1 : expr2)
├── edge-zero.glsl           (uvec3(0u, 0u, 0u) patterns)
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

uvec3 test_uvec_operation_name() {
    // Test implementation
    return result;
    // Should be uvec3(expected_x, expected_y, expected_z)
}

// run: test_uvec_operation_name() == uvec3(expected_x, expected_y, expected_z)
```

## Key Test Categories

### 1. Arithmetic Operators

**op-add.glsl**: Test `+` operator (component-wise)
- `uvec3 + uvec3` → `uvec3` (component-wise addition)
- Test wraparound behavior (modulo 2^32)

**op-subtract.glsl**: Test `-` operator (component-wise)
- `uvec3 - uvec3` → `uvec3` (component-wise subtraction)
- Test wraparound behavior (modulo 2^32)

**op-multiply.glsl**: Test `*` operator (component-wise)
- `uvec3 * uvec3` → `uvec3` (component-wise multiplication)
- Test wraparound behavior (modulo 2^32)

**op-divide.glsl**: Test `/` operator (component-wise)
- `uvec3 / uvec3` → `uvec3` (component-wise division, truncates)
- Test division by zero (undefined behavior)

**op-modulo.glsl**: Test `%` operator (component-wise)
- `uvec3 % uvec3` → `uvec3` (component-wise modulo)
- Test modulo by zero (undefined behavior)

### 2. Comparison Operators

**op-equal.glsl**: Test `==` operator (aggregate comparison)
- `uvec3 == uvec3` → `bool` (true if all components equal)

**op-not-equal.glsl**: Test `!=` operator (aggregate comparison)
- `uvec3 != uvec3` → `bool` (true if any component differs)

### 3. Unary Operators

**op-unary-plus.glsl**: Test `+` unary operator
- `+uvec3` → `uvec3` (no-op for uvec3)

### 4. Increment/Decrement Operators

**op-increment-pre.glsl**: Test `++` pre-increment
- `++uvec3` → `uvec3` (increment all components, return new value)

**op-increment-post.glsl**: Test `++` post-increment
- `uvec3++` → `uvec3` (return old value, then increment all components)

**op-decrement-pre.glsl**: Test `--` pre-decrement
- `--uvec3` → `uvec3` (decrement all components, return new value)

**op-decrement-post.glsl**: Test `--` post-decrement
- `uvec3--` → `uvec3` (return old value, then decrement all components)

### 5. Built-in Functions

**fn-less-than.glsl**: Test `lessThan()` built-in
- `lessThan(uvec3, uvec3)` → `bvec3` (component-wise comparison)

**fn-greater-than.glsl**: Test `greaterThan()` built-in
- `greaterThan(uvec3, uvec3)` → `bvec3` (component-wise comparison)

**fn-less-equal.glsl**: Test `lessThanEqual()` built-in
- `lessThanEqual(uvec3, uvec3)` → `bvec3` (component-wise comparison)

**fn-greater-equal.glsl**: Test `greaterThanEqual()` built-in
- `greaterThanEqual(uvec3, uvec3)` → `bvec3` (component-wise comparison)

**fn-equal.glsl**: Test `equal()` built-in
- `equal(uvec3, uvec3)` → `bvec3` (component-wise equality)

**fn-not-equal.glsl**: Test `notEqual()` built-in
- `notEqual(uvec3, uvec3)` → `bvec3` (component-wise inequality)

**fn-min.glsl**: Test `min()` built-in
- `min(uvec3, uvec3)` → `uvec3` (component-wise minimum)

**fn-max.glsl**: Test `max()` built-in
- `max(uvec3, uvec3)` → `uvec3` (component-wise maximum)

**fn-clamp.glsl**: Test `clamp()` built-in
- `clamp(uvec3, uvec3, uvec3)` → `uvec3` (component-wise clamp)

### 6. Constructors

**from-scalar.glsl**: Test scalar broadcast constructor
- `uvec3(uint)` - broadcast single uint to all components
- `uvec3(5u)` → `uvec3(5u, 5u, 5u)`

**from-scalars.glsl**: Test constructors from multiple scalars
- `uvec3(uint, uint, uint)` - from 3 uints
- Various combinations of values

**from-vectors.glsl**: Test constructors from vector combinations
- `uvec3(uvec2, uint)` - combine uvec2 and uint
- `uvec3(uint, uvec2)` - combine uint and uvec2
- Test all valid combinations

**from-shortening.glsl**: Test shortening constructor
- `uvec3(uvec4)` - extract first three components
- Verify components are preserved in order

**from-uvec.glsl**: Test identity constructor
- `uvec3(uvec3)` - identity constructor
- Should preserve all components

**from-mixed.glsl**: Test constructors with type conversions
- `uvec3(bool, bool, bool)` - converts to uint (false → 0u, true → 1u)
- `uvec3(float, float, float)` - truncates fractional part, must be non-negative
- Test various numeric inputs

### 7. Conversions

**to-uint.glsl**: Test conversion to scalar uint
- `uint(uvec3)` - extracts first component
- `uint(uvec3(5u, 10u, 15u))` → `5u` (first component)

**to-float.glsl**: Test conversion to scalar float
- `float(uvec3)` - converts first component to float

**to-bool.glsl**: Test conversion to scalar bool
- `bool(uvec3)` - converts first component (0u → false, non-zero → true)

**to-ivec.glsl**: Test conversion to ivec3
- `ivec3(uvec3)` - component-wise conversion
- Values may overflow if > INT_MAX

**to-vec.glsl**: Test conversion to vec3
- `vec3(uvec3)` - component-wise conversion

### 8. Assignment

**assign-simple.glsl**: Test simple assignment
- `uvec3 a = uvec3(...); uvec3 b = a;` - assignment
- Verify independence (modifying one doesn't affect the other)

**assign-element.glsl**: Test single component assignment
- `uvec3.x = uint` - assign to single component by name
- `uvec3[0] = uint` - assign to single component by index
- Test all components: x, y, z and indices 0, 1, 2

**assign-swizzle.glsl**: Test multi-component swizzle assignment
- `uvec3.xy = uvec2(...)` - assign to swizzle
- `uvec3.xyz = uvec3(...)` - assign to swizzle

### 9. Component Access

**access-array.glsl**: Test array-style indexing
- `uvec3[0]`, `uvec3[1]`, `uvec3[2]` - array indexing
- Variable indexing: `uvec3[i]` where `i` is computed

**access-component.glsl**: Test component name access
- `uvec3.x`, `uvec3.y`, `uvec3.z` - component access

**access-swizzle.glsl**: Test component swizzling
- `uvec3.xy` → `uvec2`
- `uvec3.xyz` → `uvec3` (identity)
- `uvec3.zyx` → `uvec3` (reverse)
- Test various patterns including z component

### 10. Control Flow

**ctrl-if.glsl**: Test `if` statements with uvec3
- Control flow conditions must use built-ins that return `bvec3`, then `any()` or `all()`
- `if (any(lessThan(uvec3, uvec3)))` - condition using any()

**ctrl-while.glsl**: Test `while` loops with uvec3
- `while (any(greaterThan(uvec3, uvec3)))` - loop condition

**ctrl-for.glsl**: Test `for` loops with uvec3
- `for (init; any(notEqual(uvec3, uvec3)); update)` - for loop condition

**ctrl-do-while.glsl**: Test `do-while` loops with uvec3
- `do { } while (any(lessThan(uvec3, uvec3)))` - do-while condition

**ctrl-ternary.glsl**: Test ternary operator with uvec3
- `any(equal(uvec3, uvec3)) ? expr1 : expr2` - ternary with uvec3 condition

### 11. Edge Cases

**edge-zero.glsl**: Test edge case with all components zero
- `uvec3(0u, 0u, 0u)` patterns

**edge-max.glsl**: Test max uint component values
- Components with UINT_MAX (4294967295u)
- Operations at the maximum value

**edge-wraparound.glsl**: Test wraparound behavior
- Addition that exceeds UINT_MAX (wraps to 0)
- Subtraction wraparound

**edge-mixed-components.glsl**: Test various component patterns
- Different patterns: `(1u, 100u, 1000u)`, etc.

## Implementation Notes

1. **Test Format**: Follow the exact format from `vec4/` and `bool/` tests

2. **Coverage**: Ensure tests cover all operators, constructors, conversions, built-ins, component access, and control flow

3. **Key Differences from uvec2**:
   - Additional component (z) and index [2]
   - Vector combination constructors: `uvec3(uvec2, uint)`, `uvec3(uint, uvec2)`
   - Extended swizzle patterns including z component
   - Shortening constructor from uvec4 only

4. **Expected Failures**: Built-in functions, some constructors, conversions, swizzle assignment, increment/decrement

5. **Reference Files**: Same as uvec2 but adapted for 3 components

## Files to Create

Create 33 test files in the flat `uvec3/` directory structure above, with each file containing 3-10 test functions following the vec4/bool pattern.

- `op-*` for operators (arithmetic, comparison, unary, increment/decrement)
- `fn-*` for built-in functions (comparison, min/max)
- `from-*` for constructors (from-scalar, from-scalars, from-vectors, from-shortening, from-uvec, from-mixed)
- `to-*` for conversions
- `assign-*` for assignments (assign-simple, assign-element, assign-swizzle)
- `access-*` for component access (array, component, swizzle)
- `ctrl-*` for control flow
- `edge-*` for edge cases

## GLSL Spec References

- **operators.adoc**: Constructors (lines 171-229), Vector components (lines 427-579), Arithmetic operators (lines 580-700), Conversions (lines 908-1100)
- **builtinfunctions.adoc**: Relational functions (lines 1228-1312), Common functions for unsigned integer types
