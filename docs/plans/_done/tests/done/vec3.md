# Plan: Create Comprehensive vec3 Tests

## Overview

Create a complete test suite for float vector type `vec3` in `lightplayer/crates/lp-glsl-filetests/filetests/vec3/` following the organizational pattern used in `vec4/` and `bool/`. These tests will comprehensively cover the GLSL single-precision float vector specification for `vec3` and are expected to fail initially, serving as a specification for implementing the rest of the compiler.

## Directory Structure

Following the flat naming convention with prefixes (like `vec4/` and `bool/`), create tests in a single `vec3/` directory:

```javascript
vec3/
├── op-add.glsl              (vec3 + vec3 -> vec3, component-wise)
├── op-subtract.glsl         (vec3 - vec3 -> vec3, component-wise)
├── op-multiply.glsl         (vec3 * vec3 -> vec3, component-wise)
├── op-divide.glsl           (vec3 / vec3 -> vec3, component-wise)
├── op-equal.glsl            (vec3 == vec3 -> bool, aggregate comparison)
├── op-not-equal.glsl        (vec3 != vec3 -> bool, aggregate comparison)
├── op-unary-minus.glsl      (-vec3 -> vec3, component-wise negation)
├── op-increment-pre.glsl    (++vec3 -> vec3, pre-increment)
├── op-increment-post.glsl   (vec3++ -> vec3, post-increment)
├── op-decrement-pre.glsl    (--vec3 -> vec3, pre-decrement)
├── op-decrement-post.glsl   (vec3-- -> vec3, post-decrement)
├── fn-less-than.glsl        (lessThan(vec3, vec3) -> bvec3)
├── fn-greater-than.glsl     (greaterThan(vec3, vec3) -> bvec3)
├── fn-less-equal.glsl       (lessThanEqual(vec3, vec3) -> bvec3)
├── fn-greater-equal.glsl    (greaterThanEqual(vec3, vec3) -> bvec3)
├── fn-equal.glsl            (equal(vec3, vec3) -> bvec3, component-wise)
├── fn-not-equal.glsl        (notEqual(vec3, vec3) -> bvec3, component-wise)
├── fn-length.glsl           (length(vec3) -> float)
├── fn-distance.glsl         (distance(vec3, vec3) -> float)
├── fn-dot.glsl              (dot(vec3, vec3) -> float)
├── fn-cross.glsl            (cross(vec3, vec3) -> vec3)
├── fn-normalize.glsl        (normalize(vec3) -> vec3)
├── fn-min.glsl              (min(vec3, vec3) -> vec3, component-wise)
├── fn-max.glsl              (max(vec3, vec3) -> vec3, component-wise)
├── fn-clamp.glsl            (clamp(vec3, vec3, vec3) -> vec3, component-wise)
├── fn-mix.glsl              (mix(vec3, vec3, vec3) -> vec3, component-wise)
├── fn-step.glsl             (step(vec3, vec3) -> vec3, component-wise)
├── fn-smoothstep.glsl       (smoothstep(vec3, vec3, vec3) -> vec3, component-wise)
├── fn-abs.glsl              (abs(vec3) -> vec3, component-wise)
├── from-scalar.glsl         (vec3(float) - broadcast)
├── from-scalars.glsl        (vec3(float, float, float))
├── from-vectors.glsl        (vec3(vec2, float), vec3(float, vec2) - vector combinations)
├── from-shortening.glsl     (vec3(vec4) - shortening)
├── from-vec.glsl            (vec3(vec3) - identity)
├── from-mixed.glsl          (vec3(int, int, int), vec3(bool, bool, bool) - conversions)
├── to-float.glsl            (float(vec3) - extract first component)
├── to-int.glsl              (int(vec3) - extract first component)
├── to-uint.glsl             (uint(vec3) - extract first component)
├── to-bool.glsl             (bool(vec3) - extract first component)
├── to-ivec.glsl             (ivec3(vec3) - component-wise conversion)
├── to-uvec.glsl             (uvec3(vec3) - component-wise conversion)
├── assign-simple.glsl       (vec3 = vec3)
├── assign-element.glsl      (vec3.x = float, vec3[0] = float - single component)
├── assign-swizzle.glsl      (vec3.xy = vec2, vec3.xyz = vec3 - multi-component swizzle)
├── access-array.glsl        (vec3[0], vec3[1], vec3[2])
├── access-component.glsl    (vec3.x, vec3.y, vec3.z)
├── access-swizzle.glsl      (vec3.xy, vec3.xyz, vec3.zyx, etc.)
├── ctrl-if.glsl             (if (any(bvec_from_comparison)) - control flow)
├── ctrl-while.glsl          (while (any(bvec_from_comparison)))
├── ctrl-for.glsl            (for (init; any(bvec_from_comparison); update))
├── ctrl-do-while.glsl       (do { } while (any(bvec_from_comparison)))
├── ctrl-ternary.glsl        (any(bvec_from_comparison) ? expr1 : expr2)
├── edge-zero.glsl           (vec3(0.0, 0.0, 0.0) patterns)
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

vec3 test_vec_operation_name() {
    // Test implementation
    return result;
    // Should be vec3(expected_x, expected_y, expected_z)
}

// run: test_vec_operation_name() ~= vec3(expected_x, expected_y, expected_z)
```

Note: Use `~=` for approximate equality due to floating-point precision issues.

## Key Test Categories

### 1. Arithmetic Operators

**op-add.glsl**: Test `+` operator (component-wise)
- `vec3 + vec3` → `vec3` (component-wise addition)
- Test with positive/negative numbers, NaN, Inf

**op-subtract.glsl**: Test `-` operator (component-wise)
- `vec3 - vec3` → `vec3` (component-wise subtraction)
- Test with positive/negative numbers

**op-multiply.glsl**: Test `*` operator (component-wise)
- `vec3 * vec3` → `vec3` (component-wise multiplication)
- Test with positive/negative numbers, zero, Inf

**op-divide.glsl**: Test `/` operator (component-wise)
- `vec3 / vec3` → `vec3` (component-wise division)
- Test division by zero (produces Inf)
- Test division involving NaN/Inf

### 2. Comparison Operators

**op-equal.glsl**: Test `==` operator (aggregate comparison)
- `vec3 == vec3` → `bool` (true if all components equal)
- Test with matching vectors, partially matching, completely different

**op-not-equal.glsl**: Test `!=` operator (aggregate comparison)
- `vec3 != vec3` → `bool` (true if any component differs)

### 3. Unary Operators

**op-unary-minus.glsl**: Test `-` unary operator (component-wise)
- `-vec3` → `vec3` (component-wise negation)
- Test with positive/negative values, NaN, Inf

### 4. Increment/Decrement Operators

**op-increment-pre.glsl**: Test `++` pre-increment
- `++vec3` → `vec3` (increment all components, return new value)
- Must be on lvalue

**op-increment-post.glsl**: Test `++` post-increment
- `vec3++` → `vec3` (return old value, then increment all components)
- Must be on lvalue

**op-decrement-pre.glsl**: Test `--` pre-decrement
- `--vec3` → `vec3` (decrement all components, return new value)
- Must be on lvalue

**op-decrement-post.glsl**: Test `--` post-decrement
- `vec3--` → `vec3` (return old value, then decrement all components)
- Must be on lvalue

### 5. Built-in Functions

**fn-less-than.glsl**: Test `lessThan()` built-in
- `lessThan(vec3, vec3)` → `bvec3` (component-wise comparison)

**fn-greater-than.glsl**: Test `greaterThan()` built-in
- `greaterThan(vec3, vec3)` → `bvec3` (component-wise comparison)

**fn-less-equal.glsl**: Test `lessThanEqual()` built-in
- `lessThanEqual(vec3, vec3)` → `bvec3` (component-wise comparison)

**fn-greater-equal.glsl**: Test `greaterThanEqual()` built-in
- `greaterThanEqual(vec3, vec3)` → `bvec3` (component-wise comparison)

**fn-equal.glsl**: Test `equal()` built-in
- `equal(vec3, vec3)` → `bvec3` (component-wise equality)

**fn-not-equal.glsl**: Test `notEqual()` built-in
- `notEqual(vec3, vec3)` → `bvec3` (component-wise inequality)

**fn-length.glsl**: Test `length()` built-in
- `length(vec3)` → `float` (Euclidean length)

**fn-distance.glsl**: Test `distance()` built-in
- `distance(vec3, vec3)` → `float` (distance between points)

**fn-dot.glsl**: Test `dot()` built-in
- `dot(vec3, vec3)` → `float` (dot product)

**fn-cross.glsl**: Test `cross()` built-in
- `cross(vec3, vec3)` → `vec3` (cross product)
- Test right-hand rule
- Test properties: anti-commutative, etc.

**fn-normalize.glsl**: Test `normalize()` built-in
- `normalize(vec3)` → `vec3` (normalize to unit length)

**fn-min.glsl**: Test `min()` built-in
- `min(vec3, vec3)` → `vec3` (component-wise minimum)

**fn-max.glsl**: Test `max()` built-in
- `max(vec3, vec3)` → `vec3` (component-wise maximum)

**fn-clamp.glsl**: Test `clamp()` built-in
- `clamp(vec3, vec3, vec3)` → `vec3` (component-wise clamp)

**fn-mix.glsl**: Test `mix()` built-in
- `mix(vec3, vec3, vec3)` → `vec3` (component-wise linear interpolation)

**fn-step.glsl**: Test `step()` built-in
- `step(vec3, vec3)` → `vec3` (component-wise step function)

**fn-smoothstep.glsl**: Test `smoothstep()` built-in
- `smoothstep(vec3, vec3, vec3)` → `vec3` (component-wise smooth step)

**fn-abs.glsl**: Test `abs()` built-in
- `abs(vec3)` → `vec3` (component-wise absolute value)

### 6. Constructors

**from-scalar.glsl**: Test scalar broadcast constructor
- `vec3(float)` - broadcast single float to all components
- `vec3(5.0)` → `vec3(5.0, 5.0, 5.0)`

**from-scalars.glsl**: Test constructors from multiple scalars
- `vec3(float, float, float)` - from 3 floats
- Various combinations of values

**from-vectors.glsl**: Test constructors from vector combinations
- `vec3(vec2, float)` - combine vec2 and float
- `vec3(float, vec2)` - combine float and vec2
- Test all valid combinations

**from-shortening.glsl**: Test shortening constructor
- `vec3(vec4)` - extract first three components
- Verify components are preserved in order

**from-vec.glsl**: Test identity constructor
- `vec3(vec3)` - identity constructor
- Should preserve all components

**from-mixed.glsl**: Test constructors with type conversions
- `vec3(int, int, int)` - converts to float
- `vec3(bool, bool, bool)` - converts to float (false → 0.0, true → 1.0)
- Test various numeric inputs

### 7. Conversions

**to-float.glsl**: Test conversion to scalar float
- `float(vec3)` - extracts first component
- `float(vec3(5.0, 10.0, 15.0))` → `5.0` (first component)

**to-int.glsl**: Test conversion to scalar int
- `int(vec3)` - converts first component (truncates)
- Test with positive/negative floats, NaN, Inf

**to-uint.glsl**: Test conversion to scalar uint
- `uint(vec3)` - converts first component (truncates)
- Test with positive floats, NaN, Inf, negative values

**to-bool.glsl**: Test conversion to scalar bool
- `bool(vec3)` - converts first component (0.0 → false, non-zero → true)
- Test with NaN (NaN → true)

**to-ivec.glsl**: Test conversion to ivec3
- `ivec3(vec3)` - component-wise conversion (truncates)
- Test with positive/negative floats, NaN, Inf

**to-uvec.glsl**: Test conversion to uvec3
- `uvec3(vec3)` - component-wise conversion (truncates)
- Test with positive floats, NaN, Inf, negative values

### 8. Assignment

**assign-simple.glsl**: Test simple assignment
- `vec3 a = vec3(...); vec3 b = a;` - assignment
- Verify independence (modifying one doesn't affect the other)
- Self-assignment: `vec3 = vec3`

**assign-element.glsl**: Test single component assignment
- `vec3.x = float` - assign to single component by name
- `vec3[0] = float` - assign to single component by index
- Verify other components unchanged
- Test all components: x, y, z and indices 0, 1, 2

**assign-swizzle.glsl**: Test multi-component swizzle assignment
- `vec3.xy = vec2(...)` - assign to swizzle
- `vec3.xyz = vec3(...)` - assign to swizzle
- Verify components are updated correctly
- Test various swizzle patterns (no duplicates allowed in assignment)

### 9. Component Access

**access-array.glsl**: Test array-style indexing
- `vec3[0]`, `vec3[1]`, `vec3[2]` - array indexing
- Variable indexing: `vec3[i]` where `i` is computed
- Verify correct component access

**access-component.glsl**: Test component name access
- `vec3.x`, `vec3.y`, `vec3.z` - component access
- Verify correct component values

**access-swizzle.glsl**: Test component swizzling
- `vec3.xy` → `vec2`
- `vec3.xyz` → `vec3` (identity)
- `vec3.zyx` → `vec3` (reverse)
- `vec3.xxy` → `vec3` (duplicate)
- Test all name sets: `xyz`, `rgb`, `stp`
- Test various patterns: `xy`, `xz`, `yz`, `zyx`, etc.

### 10. Control Flow

**ctrl-if.glsl**: Test `if` statements with vec3
- Control flow conditions must be scalar `bool`, so use built-ins that return `bvec3`, then `any()` or `all()`
- `if (any(lessThan(vec3, vec3)))` - condition using any()
- `if (all(equal(vec3, vec3)))` - condition using all()

**ctrl-while.glsl**: Test `while` loops with vec3
- `while (any(greaterThan(vec3, vec3)))` - loop condition

**ctrl-for.glsl**: Test `for` loops with vec3
- `for (init; any(notEqual(vec3, vec3)); update)` - for loop condition

**ctrl-do-while.glsl**: Test `do-while` loops with vec3
- `do { } while (any(lessThan(vec3, vec3)))` - do-while condition

**ctrl-ternary.glsl**: Test ternary operator with vec3
- `any(equal(vec3, vec3)) ? expr1 : expr2` - ternary with vec3 condition

### 11. Edge Cases

**edge-zero.glsl**: Test edge case with all components zero
- `vec3(0.0, 0.0, 0.0)` patterns
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
- Different patterns: `(1.0, -1.0, 5.0)`, `(100.0, 0.001, -50.0)`, etc.
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

3. **Key Differences from vec2**:
   - Additional component (z) and index [2]
   - Vector combination constructors: `vec3(vec2, float)`, `vec3(float, vec2)`
   - Extended swizzle patterns including z component
   - `cross()` product available (vec3-specific)
   - Shortening constructor from vec4 only

4. **Expected Failures**: These tests are expected to fail initially, especially:
   - Built-in functions (geometric including cross, relational, common)
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

Create 34 test files in the flat `vec3/` directory structure above, with each file containing 3-10 test functions following the vec4/bool pattern. All files use the prefix naming convention:

- `op-*` for operators (arithmetic, comparison, unary, increment/decrement)
- `fn-*` for built-in functions (comparison, geometric including cross, min/max, interpolation)
- `from-*` for constructors (from-scalar, from-scalars, from-vectors, from-shortening, from-vec, from-mixed)
- `to-*` for conversions
- `assign-*` for assignments (assign-simple, assign-element, assign-swizzle)
- `access-*` for component access (array, component, swizzle)
- `ctrl-*` for control flow
- `edge-*` for edge cases

## GLSL Spec References

- **operators.adoc**: Constructors (lines 171-229), Vector components (lines 427-579), Arithmetic operators (lines 580-700), Conversions (lines 908-1100)
- **builtinfunctions.adoc**: Geometric functions (lines 1313-1450), Relational functions (lines 1228-1312), Common functions for `GenFType`
