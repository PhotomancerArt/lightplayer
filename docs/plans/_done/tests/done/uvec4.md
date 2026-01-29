# Plan: Create Comprehensive uvec4 Tests

## Overview

Create a complete test suite for unsigned integer vector type `uvec4` in `lightplayer/crates/lp-glsl-filetests/filetests/uvec4/` following the organizational pattern used in `vec4/` and `bool/`. These tests will comprehensively cover the GLSL unsigned integer vector specification for `uvec4` and are expected to fail initially, serving as a specification for implementing the rest of the compiler.

## Directory Structure

Following the flat naming convention with prefixes (like `vec4/` and `bool/`), create tests in a single `uvec4/` directory:

```javascript
uvec4/
├── op-add.glsl              (uvec4 + uvec4 -> uvec4, component-wise)
├── op-subtract.glsl         (uvec4 - uvec4 -> uvec4, component-wise)
├── op-multiply.glsl         (uvec4 * uvec4 -> uvec4, component-wise)
├── op-divide.glsl           (uvec4 / uvec4 -> uvec4, component-wise, truncates)
├── op-modulo.glsl           (uvec4 % uvec4 -> uvec4, component-wise)
├── op-equal.glsl            (uvec4 == uvec4 -> bool, aggregate comparison)
├── op-not-equal.glsl        (uvec4 != uvec4 -> bool, aggregate comparison)
├── op-unary-plus.glsl       (+uvec4 -> uvec4)
├── op-increment-pre.glsl    (++uvec4 -> uvec4, pre-increment)
├── op-increment-post.glsl   (uvec4++ -> uvec4, post-increment)
├── op-decrement-pre.glsl    (--uvec4 -> uvec4, pre-decrement)
├── op-decrement-post.glsl   (uvec4-- -> uvec4, post-decrement)
├── fn-less-than.glsl        (lessThan(uvec4, uvec4) -> bvec4)
├── fn-greater-than.glsl     (greaterThan(uvec4, uvec4) -> bvec4)
├── fn-less-equal.glsl       (lessThanEqual(uvec4, uvec4) -> bvec4)
├── fn-greater-equal.glsl    (greaterThanEqual(uvec4, uvec4) -> bvec4)
├── fn-equal.glsl            (equal(uvec4, uvec4) -> bvec4, component-wise)
├── fn-not-equal.glsl        (notEqual(uvec4, uvec4) -> bvec4, component-wise)
├── fn-min.glsl              (min(uvec4, uvec4) -> uvec4, component-wise)
├── fn-max.glsl              (max(uvec4, uvec4) -> uvec4, component-wise)
├── fn-clamp.glsl            (clamp(uvec4, uvec4, uvec4) -> uvec4, component-wise)
├── from-scalar.glsl         (uvec4(uint) - broadcast)
├── from-scalars.glsl        (uvec4(uint, uint, uint, uint))
├── from-vectors.glsl        (uvec4(uvec2, uvec2), uvec4(uvec3, uint), etc. - vector combinations)
├── from-uvec.glsl           (uvec4(uvec4) - identity)
├── from-mixed.glsl          (uvec4(bool, bool, bool, bool), uvec4(float, float, float, float) - conversions)
├── to-uint.glsl             (uint(uvec4) - extract first component)
├── to-float.glsl            (float(uvec4) - extract first component)
├── to-bool.glsl             (bool(uvec4) - extract first component)
├── to-ivec.glsl             (ivec4(uvec4) - component-wise conversion)
├── to-vec.glsl              (vec4(uvec4) - component-wise conversion)
├── assign-simple.glsl       (uvec4 = uvec4)
├── assign-element.glsl      (uvec4.x = uint, uvec4[0] = uint - single component)
├── assign-swizzle.glsl      (uvec4.xy = uvec2, uvec4.xyz = uvec3 - multi-component swizzle)
├── access-array.glsl        (uvec4[0], uvec4[1], uvec4[2], uvec4[3])
├── access-component.glsl    (uvec4.x, uvec4.y, uvec4.z, uvec4.w)
├── access-swizzle.glsl      (uvec4.xy, uvec4.xyz, uvec4.xyzw, uvec4.wzyx, etc.)
├── ctrl-if.glsl             (if (any(bvec_from_comparison)) - control flow)
├── ctrl-while.glsl          (while (any(bvec_from_comparison)))
├── ctrl-for.glsl            (for (init; any(bvec_from_comparison); update))
├── ctrl-do-while.glsl       (do { } while (any(bvec_from_comparison)))
├── ctrl-ternary.glsl        (any(bvec_from_comparison) ? expr1 : expr2)
├── edge-zero.glsl           (uvec4(0u, 0u, 0u, 0u) patterns)
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

uvec4 test_uvec_operation_name() {
    // Test implementation
    return result;
    // Should be uvec4(expected_x, expected_y, expected_z, expected_w)
}

// run: test_uvec_operation_name() == uvec4(expected_x, expected_y, expected_z, expected_w)
```

## Key Test Categories

### 1. Arithmetic Operators

**op-add.glsl**: Test `+` operator (component-wise)
- `uvec4 + uvec4` → `uvec4` (component-wise addition)
- Test wraparound behavior (modulo 2^32)

**op-subtract.glsl**: Test `-` operator (component-wise)
- `uvec4 - uvec4` → `uvec4` (component-wise subtraction)
- Test wraparound behavior (modulo 2^32)

**op-multiply.glsl**: Test `*` operator (component-wise)
- `uvec4 * uvec4` → `uvec4` (component-wise multiplication)
- Test wraparound behavior (modulo 2^32)

**op-divide.glsl**: Test `/` operator (component-wise)
- `uvec4 / uvec4` → `uvec4` (component-wise division, truncates)
- Test division by zero (undefined behavior)

**op-modulo.glsl**: Test `%` operator (component-wise)
- `uvec4 % uvec4` → `uvec4` (component-wise modulo)
- Test modulo by zero (undefined behavior)

### 2. Comparison Operators

**op-equal.glsl**: Test `==` operator (aggregate comparison)
- `uvec4 == uvec4` → `bool` (true if all components equal)

**op-not-equal.glsl**: Test `!=` operator (aggregate comparison)
- `uvec4 != uvec4` → `bool` (true if any component differs)

### 3. Unary Operators

**op-unary-plus.glsl**: Test `+` unary operator
- `+uvec4` → `uvec4` (no-op for uvec4)

### 4. Increment/Decrement Operators

**op-increment-pre.glsl**: Test `++` pre-increment
- `++uvec4` → `uvec4` (increment all components, return new value)

**op-increment-post.glsl**: Test `++` post-increment
- `uvec4++` → `uvec4` (return old value, then increment all components)

**op-decrement-pre.glsl**: Test `--` pre-decrement
- `--uvec4` → `uvec4` (decrement all components, return new value)

**op-decrement-post.glsl**: Test `--` post-decrement
- `uvec4--` → `uvec4` (return old value, then decrement all components)

### 5. Built-in Functions

**fn-less-than.glsl**: Test `lessThan()` built-in
- `lessThan(uvec4, uvec4)` → `bvec4` (component-wise comparison)

**fn-greater-than.glsl**: Test `greaterThan()` built-in
- `greaterThan(uvec4, uvec4)` → `bvec4` (component-wise comparison)

**fn-less-equal.glsl**: Test `lessThanEqual()` built-in
- `lessThanEqual(uvec4, uvec4)` → `bvec4` (component-wise comparison)

**fn-greater-equal.glsl**: Test `greaterThanEqual()` built-in
- `greaterThanEqual(uvec4, uvec4)` → `bvec4` (component-wise comparison)

**fn-equal.glsl**: Test `equal()` built-in
- `equal(uvec4, uvec4)` → `bvec4` (component-wise equality)

**fn-not-equal.glsl**: Test `notEqual()` built-in
- `notEqual(uvec4, uvec4)` → `bvec4` (component-wise inequality)

**fn-min.glsl**: Test `min()` built-in
- `min(uvec4, uvec4)` → `uvec4` (component-wise minimum)

**fn-max.glsl**: Test `max()` built-in
- `max(uvec4, uvec4)` → `uvec4` (component-wise maximum)

**fn-clamp.glsl**: Test `clamp()` built-in
- `clamp(uvec4, uvec4, uvec4)` → `uvec4` (component-wise clamp)

### 6. Constructors

**from-scalar.glsl**: Test scalar broadcast constructor
- `uvec4(uint)` - broadcast single uint to all components
- `uvec4(5u)` → `uvec4(5u, 5u, 5u, 5u)`

**from-scalars.glsl**: Test constructors from multiple scalars
- `uvec4(uint, uint, uint, uint)` - from 4 uints
- Various combinations of values

**from-vectors.glsl**: Test constructors from vector combinations
- `uvec4(uvec2, uvec2)` - combine two uvec2s
- `uvec4(uvec3, uint)` - combine uvec3 and uint
- `uvec4(uint, uvec3)` - combine uint and uvec3
- `uvec4(uvec2, uint, uint)` - various combinations
- Test all valid combinations for constructing uvec4

**from-uvec.glsl**: Test identity constructor
- `uvec4(uvec4)` - identity constructor
- Should preserve all components

**from-mixed.glsl**: Test constructors with type conversions
- `uvec4(bool, bool, bool, bool)` - converts to uint (false → 0u, true → 1u)
- `uvec4(float, float, float, float)` - truncates fractional part, must be non-negative
- Test various numeric inputs

### 7. Conversions

**to-uint.glsl**: Test conversion to scalar uint
- `uint(uvec4)` - extracts first component
- `uint(uvec4(5u, 10u, 15u, 20u))` → `5u` (first component)

**to-float.glsl**: Test conversion to scalar float
- `float(uvec4)` - converts first component to float

**to-bool.glsl**: Test conversion to scalar bool
- `bool(uvec4)` - converts first component (0u → false, non-zero → true)

**to-ivec.glsl**: Test conversion to ivec4
- `ivec4(uvec4)` - component-wise conversion
- Values may overflow if > INT_MAX

**to-vec.glsl**: Test conversion to vec4
- `vec4(uvec4)` - component-wise conversion

### 8. Assignment

**assign-simple.glsl**: Test simple assignment
- `uvec4 a = uvec4(...); uvec4 b = a;` - assignment
- Verify independence (modifying one doesn't affect the other)

**assign-element.glsl**: Test single component assignment
- `uvec4.x = uint` - assign to single component by name
- `uvec4[0] = uint` - assign to single component by index
- Test all components: x, y, z, w and indices 0, 1, 2, 3

**assign-swizzle.glsl**: Test multi-component swizzle assignment
- `uvec4.xy = uvec2(...)` - assign to swizzle
- `uvec4.xyz = uvec3(...)` - assign to swizzle

### 9. Component Access

**access-array.glsl**: Test array-style indexing
- `uvec4[0]`, `uvec4[1]`, `uvec4[2]`, `uvec4[3]` - array indexing
- Variable indexing: `uvec4[i]` where `i` is computed

**access-component.glsl**: Test component name access
- `uvec4.x`, `uvec4.y`, `uvec4.z`, `uvec4.w` - component access

**access-swizzle.glsl**: Test component swizzling
- `uvec4.xy` → `uvec2`
- `uvec4.xyz` → `uvec3`
- `uvec4.xyzw` → `uvec4` (identity)
- `uvec4.wzyx` → `uvec4` (reverse)
- Test various patterns including w component

### 10. Control Flow

**ctrl-if.glsl**: Test `if` statements with uvec4
- Control flow conditions must use built-ins that return `bvec4`, then `any()` or `all()`
- `if (any(lessThan(uvec4, uvec4)))` - condition using any()

**ctrl-while.glsl**: Test `while` loops with uvec4
- `while (any(greaterThan(uvec4, uvec4)))` - loop condition

**ctrl-for.glsl**: Test `for` loops with uvec4
- `for (init; any(notEqual(uvec4, uvec4)); update)` - for loop condition

**ctrl-do-while.glsl**: Test `do-while` loops with uvec4
- `do { } while (any(lessThan(uvec4, uvec4)))` - do-while condition

**ctrl-ternary.glsl**: Test ternary operator with uvec4
- `any(equal(uvec4, uvec4)) ? expr1 : expr2` - ternary with uvec4 condition

### 11. Edge Cases

**edge-zero.glsl**: Test edge case with all components zero
- `uvec4(0u, 0u, 0u, 0u)` patterns

**edge-max.glsl**: Test max uint component values
- Components with UINT_MAX (4294967295u)
- Operations at the maximum value

**edge-wraparound.glsl**: Test wraparound behavior
- Addition that exceeds UINT_MAX (wraps to 0)
- Subtraction wraparound

**edge-mixed-components.glsl**: Test various component patterns
- Different patterns: `(1u, 100u, 1000u, 10000u)`, etc.

## Implementation Notes

1. **Test Format**: Follow the exact format from `vec4/` and `bool/` tests

2. **Coverage**: Ensure tests cover all operators, constructors, conversions, built-ins, component access, and control flow

3. **Key Differences from uvec3**:
   - Additional component (w) and index [3]
   - More vector constructor combinations: `uvec4(uvec2, uvec2)`, `uvec4(uvec3, uint)`, etc.
   - Extended swizzle patterns including w component
   - No shortening constructor (uvec4 is the largest unsigned integer vector type)

4. **Expected Failures**: Built-in functions, some constructors, conversions, swizzle assignment, increment/decrement

5. **Reference Files**: Same as uvec2 but adapted for 4 components

## Files to Create

Create 33 test files in the flat `uvec4/` directory structure above, with each file containing 3-10 test functions following the vec4/bool pattern.

- `op-*` for operators (arithmetic, comparison, unary, increment/decrement)
- `fn-*` for built-in functions (comparison, min/max)
- `from-*` for constructors (from-scalar, from-scalars, from-vectors, from-uvec, from-mixed)
- `to-*` for conversions
- `assign-*` for assignments (assign-simple, assign-element, assign-swizzle)
- `access-*` for component access (array, component, swizzle)
- `ctrl-*` for control flow
- `edge-*` for edge cases

## GLSL Spec References

- **operators.adoc**: Constructors (lines 171-229), Vector components (lines 427-579), Arithmetic operators (lines 580-700), Conversions (lines 908-1100)
- **builtinfunctions.adoc**: Relational functions (lines 1228-1312), Common functions for unsigned integer types
