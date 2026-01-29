# Plan: Create Comprehensive vec2 Tests

## Overview

Create a complete test suite for float vector type `vec2` in `lightplayer/crates/lp-glsl-filetests/filetests/vec2/` following the organizational pattern used in `vec4/` and `bool/`. These tests will comprehensively cover the GLSL single-precision float vector specification for `vec2` and are expected to fail initially, serving as a specification for implementing the rest of the compiler.

## Directory Structure

Following the flat naming convention with prefixes (like `vec4/` and `bool/`), create tests in a single `vec2/` directory:

```javascript
vec2/
├── op-add.glsl              (vec2 + vec2 -> vec2, component-wise)
├── op-subtract.glsl         (vec2 - vec2 -> vec2, component-wise)
├── op-multiply.glsl         (vec2 * vec2 -> vec2, component-wise)
├── op-divide.glsl           (vec2 / vec2 -> vec2, component-wise)
├── op-equal.glsl            (vec2 == vec2 -> bool, aggregate comparison)
├── op-not-equal.glsl        (vec2 != vec2 -> bool, aggregate comparison)
├── op-unary-minus.glsl      (-vec2 -> vec2, component-wise negation)
├── op-increment-pre.glsl    (++vec2 -> vec2, pre-increment)
├── op-increment-post.glsl   (vec2++ -> vec2, post-increment)
├── op-decrement-pre.glsl    (--vec2 -> vec2, pre-decrement)
├── op-decrement-post.glsl   (vec2-- -> vec2, post-decrement)
├── fn-less-than.glsl        (lessThan(vec2, vec2) -> bvec2)
├── fn-greater-than.glsl     (greaterThan(vec2, vec2) -> bvec2)
├── fn-less-equal.glsl       (lessThanEqual(vec2, vec2) -> bvec2)
├── fn-greater-equal.glsl    (greaterThanEqual(vec2, vec2) -> bvec2)
├── fn-equal.glsl            (equal(vec2, vec2) -> bvec2, component-wise)
├── fn-not-equal.glsl        (notEqual(vec2, vec2) -> bvec2, component-wise)
├── fn-length.glsl           (length(vec2) -> float)
├── fn-distance.glsl         (distance(vec2, vec2) -> float)
├── fn-dot.glsl              (dot(vec2, vec2) -> float)
├── fn-normalize.glsl        (normalize(vec2) -> vec2)
├── fn-min.glsl              (min(vec2, vec2) -> vec2, component-wise)
├── fn-max.glsl              (max(vec2, vec2) -> vec2, component-wise)
├── fn-clamp.glsl            (clamp(vec2, vec2, vec2) -> vec2, component-wise)
├── fn-mix.glsl              (mix(vec2, vec2, vec2) -> vec2, component-wise)
├── fn-step.glsl             (step(vec2, vec2) -> vec2, component-wise)
├── fn-smoothstep.glsl       (smoothstep(vec2, vec2, vec2) -> vec2, component-wise)
├── fn-abs.glsl              (abs(vec2) -> vec2, component-wise)
├── from-scalar.glsl         (vec2(float) - broadcast)
├── from-scalars.glsl        (vec2(float, float))
├── from-shortening.glsl     (vec2(vec3), vec2(vec4) - shortening)
├── from-vec.glsl            (vec2(vec2) - identity)
├── from-mixed.glsl          (vec2(int, int), vec2(bool, bool) - conversions)
├── from-vectors.glsl        (vec2(vec2) - identity)
├── to-float.glsl            (float(vec2) - extract first component)
├── to-int.glsl              (int(vec2) - extract first component)
├── to-uint.glsl             (uint(vec2) - extract first component)
├── to-bool.glsl             (bool(vec2) - extract first component)
├── to-ivec.glsl             (ivec2(vec2) - component-wise conversion)
├── to-uvec.glsl             (uvec2(vec2) - component-wise conversion)
├── assign-simple.glsl       (vec2 = vec2)
├── assign-element.glsl      (vec2.x = float, vec2[0] = float - single component)
├── assign-swizzle.glsl      (vec2.xy = vec2 - multi-component swizzle)
├── access-array.glsl        (vec2[0], vec2[1])
├── access-component.glsl    (vec2.x, vec2.y)
├── access-swizzle.glsl      (vec2.xy, vec2.yx, vec2.xx, etc.)
├── ctrl-if.glsl             (if (any(bvec_from_comparison)) - control flow)
├── ctrl-while.glsl          (while (any(bvec_from_comparison)))
├── ctrl-for.glsl            (for (init; any(bvec_from_comparison); update))
├── ctrl-do-while.glsl       (do { } while (any(bvec_from_comparison)))
├── ctrl-ternary.glsl        (any(bvec_from_comparison) ? expr1 : expr2)
├── edge-zero.glsl           (vec2(0.0, 0.0) patterns)
├── edge-nan-inf.glsl        (NaN, Inf components)
├── edge-precision.glsl      (floating-point precision)
├── edge-unit-vectors.glsl   (unit vector patterns)
└── edge-mixed-components.glsl (mixed component patterns)
```

## Test File Patterns

Each test file should follow the pattern from `vec4/` and `bool/` tests:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

vec2 test_vec_operation_name() {
    // Test implementation
    return result;
    // Should be vec2(expected_x, expected_y)
}

// run: test_vec_operation_name() ~= vec2(expected_x, expected_y)
```

Note: Use `~=` for approximate equality due to floating-point precision issues.

## Key Test Categories

### 1. Arithmetic Operators

**op-add.glsl**: Test `+` operator (component-wise)
- `vec2 + vec2` → `vec2` (component-wise addition)
- Test with positive/negative numbers, NaN, Inf

**op-subtract.glsl**: Test `-` operator (component-wise)
- `vec2 - vec2` → `vec2` (component-wise subtraction)
- Test with positive/negative numbers

**op-multiply.glsl**: Test `*` operator (component-wise)
- `vec2 * vec2` → `vec2` (component-wise multiplication)
- Test with positive/negative numbers, zero, Inf

**op-divide.glsl**: Test `/` operator (component-wise)
- `vec2 / vec2` → `vec2` (component-wise division)
- Test division by zero (produces Inf)
- Test division involving NaN/Inf

### 2. Comparison Operators

**op-equal.glsl**: Test `==` operator (aggregate comparison)
- `vec2 == vec2` → `bool` (true if all components equal)
- Test with matching vectors, partially matching, completely different

**op-not-equal.glsl**: Test `!=` operator (aggregate comparison)
- `vec2 != vec2` → `bool` (true if any component differs)

### 3. Unary Operators

**op-unary-minus.glsl**: Test `-` unary operator (component-wise)
- `-vec2` → `vec2` (component-wise negation)
- Test with positive/negative values, NaN, Inf

### 4. Increment/Decrement Operators

**op-increment-pre.glsl**: Test `++` pre-increment
- `++vec2` → `vec2` (increment all components, return new value)
- Must be on lvalue

**op-increment-post.glsl**: Test `++` post-increment
- `vec2++` → `vec2` (return old value, then increment all components)
- Must be on lvalue

**op-decrement-pre.glsl**: Test `--` pre-decrement
- `--vec2` → `vec2` (decrement all components, return new value)
- Must be on lvalue

**op-decrement-post.glsl**: Test `--` post-decrement
- `vec2--` → `vec2` (return old value, then decrement all components)
- Must be on lvalue

### 5. Built-in Functions

**fn-less-than.glsl**: Test `lessThan()` built-in
- `lessThan(vec2, vec2)` → `bvec2` (component-wise comparison)

**fn-greater-than.glsl**: Test `greaterThan()` built-in
- `greaterThan(vec2, vec2)` → `bvec2` (component-wise comparison)

**fn-less-equal.glsl**: Test `lessThanEqual()` built-in
- `lessThanEqual(vec2, vec2)` → `bvec2` (component-wise comparison)

**fn-greater-equal.glsl**: Test `greaterThanEqual()` built-in
- `greaterThanEqual(vec2, vec2)` → `bvec2` (component-wise comparison)

**fn-equal.glsl**: Test `equal()` built-in
- `equal(vec2, vec2)` → `bvec2` (component-wise equality)

**fn-not-equal.glsl**: Test `notEqual()` built-in
- `notEqual(vec2, vec2)` → `bvec2` (component-wise inequality)

**fn-length.glsl**: Test `length()` built-in
- `length(vec2)` → `float` (Euclidean length)

**fn-distance.glsl**: Test `distance()` built-in
- `distance(vec2, vec2)` → `float` (distance between points)

**fn-dot.glsl**: Test `dot()` built-in
- `dot(vec2, vec2)` → `float` (dot product)

**fn-normalize.glsl**: Test `normalize()` built-in
- `normalize(vec2)` → `vec2` (normalize to unit length)

**fn-min.glsl**: Test `min()` built-in
- `min(vec2, vec2)` → `vec2` (component-wise minimum)

**fn-max.glsl**: Test `max()` built-in
- `max(vec2, vec2)` → `vec2` (component-wise maximum)

**fn-clamp.glsl**: Test `clamp()` built-in
- `clamp(vec2, vec2, vec2)` → `vec2` (component-wise clamp)

**fn-mix.glsl**: Test `mix()` built-in
- `mix(vec2, vec2, vec2)` → `vec2` (component-wise linear interpolation)

**fn-step.glsl**: Test `step()` built-in
- `step(vec2, vec2)` → `vec2` (component-wise step function)

**fn-smoothstep.glsl**: Test `smoothstep()` built-in
- `smoothstep(vec2, vec2, vec2)` → `vec2` (component-wise smooth step)

**fn-abs.glsl**: Test `abs()` built-in
- `abs(vec2)` → `vec2` (component-wise absolute value)

### 6. Constructors

**from-scalar.glsl**: Test scalar broadcast constructor
- `vec2(float)` - broadcast single float to both components
- `vec2(5.0)` → `vec2(5.0, 5.0)`

**from-scalars.glsl**: Test constructors from multiple scalars
- `vec2(float, float)` - from 2 floats
- Various combinations of values

**from-shortening.glsl**: Test shortening constructors
- `vec2(vec3)` - extract first two components
- `vec2(vec4)` - extract first two components
- Verify components are preserved in order

**from-vec.glsl**: Test identity constructor
- `vec2(vec2)` - identity constructor
- Should preserve all components

**from-mixed.glsl**: Test constructors with type conversions
- `vec2(int, int)` - converts to float
- `vec2(bool, bool)` - converts to float (false → 0.0, true → 1.0)
- Test various numeric inputs

**from-vectors.glsl**: Test identity constructor (alias for from-vec.glsl)

### 7. Conversions

**to-float.glsl**: Test conversion to scalar float
- `float(vec2)` - extracts first component
- `float(vec2(5.0, 10.0))` → `5.0` (first component)

**to-int.glsl**: Test conversion to scalar int
- `int(vec2)` - converts first component (truncates)
- Test with positive/negative floats, NaN, Inf

**to-uint.glsl**: Test conversion to scalar uint
- `uint(vec2)` - converts first component (truncates)
- Test with positive floats, NaN, Inf, negative values

**to-bool.glsl**: Test conversion to scalar bool
- `bool(vec2)` - converts first component (0.0 → false, non-zero → true)
- Test with NaN (NaN → true)

**to-ivec.glsl**: Test conversion to ivec2
- `ivec2(vec2)` - component-wise conversion (truncates)
- Test with positive/negative floats, NaN, Inf

**to-uvec.glsl**: Test conversion to uvec2
- `uvec2(vec2)` - component-wise conversion (truncates)
- Test with positive floats, NaN, Inf, negative values

### 8. Assignment

**assign-simple.glsl**: Test simple assignment
- `vec2 a = vec2(...); vec2 b = a;` - assignment
- Verify independence (modifying one doesn't affect the other)
- Self-assignment: `vec2 = vec2`

**assign-element.glsl**: Test single component assignment
- `vec2.x = float` - assign to single component by name
- `vec2[0] = float` - assign to single component by index
- Verify other components unchanged
- Test all components: x, y and indices 0, 1

**assign-swizzle.glsl**: Test multi-component swizzle assignment
- `vec2.xy = vec2(...)` - assign to swizzle
- Verify components are updated correctly
- Test various swizzle patterns (no duplicates allowed in assignment)

### 9. Component Access

**access-array.glsl**: Test array-style indexing
- `vec2[0]`, `vec2[1]` - array indexing
- Variable indexing: `vec2[i]` where `i` is computed
- Verify correct component access

**access-component.glsl**: Test component name access
- `vec2.x`, `vec2.y` - component access
- Verify correct component values

**access-swizzle.glsl**: Test component swizzling
- `vec2.xy` → `vec2` (identity)
- `vec2.yx` → `vec2` (reverse)
- `vec2.xx` → `vec2` (duplicate)
- `vec2.yy` → `vec2` (duplicate)
- Test all name sets: `xy`, `rg`, `st`
- Test various patterns: `xy`, `yx`, `xx`, `yy`, etc.

### 10. Control Flow

**ctrl-if.glsl**: Test `if` statements with vec2
- Control flow conditions must be scalar `bool`, so use built-ins that return `bvec2`, then `any()` or `all()`
- `if (any(lessThan(vec2, vec2)))` - condition using any()
- `if (all(equal(vec2, vec2)))` - condition using all()

**ctrl-while.glsl**: Test `while` loops with vec2
- `while (any(greaterThan(vec2, vec2)))` - loop condition

**ctrl-for.glsl**: Test `for` loops with vec2
- `for (init; any(notEqual(vec2, vec2)); update)` - for loop condition

**ctrl-do-while.glsl**: Test `do-while` loops with vec2
- `do { } while (any(lessThan(vec2, vec2)))` - do-while condition

**ctrl-ternary.glsl**: Test ternary operator with vec2
- `any(equal(vec2, vec2)) ? expr1 : expr2` - ternary with vec2 condition

### 11. Edge Cases

**edge-zero.glsl**: Test edge case with all components zero
- `vec2(0.0, 0.0)` patterns
- Operations with zero vectors

**edge-nan-inf.glsl**: Test NaN and Inf components
- Components with NaN, +Inf, -Inf
- Propagation through operations
- Special behavior in comparisons

**edge-precision.glsl**: Test floating-point precision
- Loss of precision with operations
- Approximate equality testing

**edge-unit-vectors.glsl**: Test unit vector patterns
- Vectors with length 1.0
- Normalization of various vectors

**edge-mixed-components.glsl**: Test various mixed component patterns
- Different patterns: `(1.0, -1.0)`, `(100.0, 0.001)`, etc.
- Verify component-wise operations work correctly

## Implementation Notes

1. **Test Format**: Follow the exact format from `vec4/` and `bool/` tests with:
   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results (use `~=` for approximate equality)
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:
   - All operators from GLSL spec (operators.adoc)
   - All constructor forms (operators.adoc, constructors section)
   - All conversion forms (operators.adoc, conversion section)
   - All built-in functions (builtinfunctions.adoc: GenFType functions)
   - Component access (swizzling, indexing)
   - Control flow requirements (statements.adoc: conditions must be bool, use any/all for bvec)

3. **Key Differences from vec4**:
   - Fewer components (2 vs 4)
   - Different swizzle patterns (no z, w components)
   - Different constructor combinations
   - No `cross()` product (requires vec3)
   - Same built-ins except geometric functions requiring 3+ components

4. **Expected Failures**: These tests are expected to fail initially, especially:
   - Built-in functions (geometric, relational, common)
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

Create 33 test files in the flat `vec2/` directory structure above, with each file containing 3-10 test functions following the vec4/bool pattern. All files use the prefix naming convention:

- `op-*` for operators (arithmetic, comparison, unary, increment/decrement)
- `fn-*` for built-in functions (comparison, geometric, min/max, interpolation)
- `from-*` for constructors (from-scalar, from-scalars, from-shortening, from-vec, from-mixed, from-vectors)
- `to-*` for conversions
- `assign-*` for assignments (assign-simple, assign-element, assign-swizzle)
- `access-*` for component access (array, component, swizzle)
- `ctrl-*` for control flow
- `edge-*` for edge cases

## GLSL Spec References

- **operators.adoc**: Constructors (lines 171-229), Vector components (lines 427-579), Arithmetic operators (lines 580-700), Conversions (lines 908-1100)
- **builtinfunctions.adoc**: Geometric functions (lines 1313-1450), Relational functions (lines 1228-1312), Common functions for `GenFType`
