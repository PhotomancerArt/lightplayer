# Plan: Create Comprehensive vec4 Tests (Replacing Existing)

## Overview

Create a complete test suite for float vector type `vec4` in `lightplayer/crates/lp-glsl-filetests/filetests/vec4/` following the flat naming convention with prefixes (like `bvec2/`, `vec2/`, `vec3/`). These tests will comprehensively cover the GLSL single-precision float vector specification for `vec4` and replace the existing nested directory structure with a cleaner, more maintainable flat structure.

**Note**: This plan replaces the existing nested `vec4/` test structure (with subdirectories like `arithmetic/`, `assignment/`, `builtins/`, etc.) with a flat structure matching the pattern used in other type-based test suites.

## Directory Structure

Following the flat naming convention with prefixes, create tests in a single `vec4/` directory:

```javascript
vec4/
├── op-add.glsl              (vec4 + vec4 -> vec4, component-wise)
├── op-subtract.glsl         (vec4 - vec4 -> vec4, component-wise)
├── op-multiply.glsl         (vec4 * vec4 -> vec4, component-wise)
├── op-divide.glsl           (vec4 / vec4 -> vec4, component-wise)
├── op-equal.glsl            (vec4 == vec4 -> bool, aggregate comparison)
├── op-not-equal.glsl        (vec4 != vec4 -> bool, aggregate comparison)
├── op-unary-minus.glsl      (-vec4 -> vec4, component-wise negation)
├── op-increment-pre.glsl    (++vec4 -> vec4, pre-increment)
├── op-increment-post.glsl   (vec4++ -> vec4, post-increment)
├── op-decrement-pre.glsl    (--vec4 -> vec4, pre-decrement)
├── op-decrement-post.glsl   (vec4-- -> vec4, post-decrement)
├── fn-less-than.glsl        (lessThan(vec4, vec4) -> bvec4)
├── fn-greater-than.glsl     (greaterThan(vec4, vec4) -> bvec4)
├── fn-less-equal.glsl       (lessThanEqual(vec4, vec4) -> bvec4)
├── fn-greater-equal.glsl    (greaterThanEqual(vec4, vec4) -> bvec4)
├── fn-equal.glsl            (equal(vec4, vec4) -> bvec4, component-wise)
├── fn-not-equal.glsl        (notEqual(vec4, vec4) -> bvec4, component-wise)
├── fn-length.glsl           (length(vec4) -> float)
├── fn-distance.glsl         (distance(vec4, vec4) -> float)
├── fn-dot.glsl              (dot(vec4, vec4) -> float)
├── fn-normalize.glsl        (normalize(vec4) -> vec4)
├── fn-faceforward.glsl      (faceforward(vec4, vec4, vec4) -> vec4)
├── fn-reflect.glsl          (reflect(vec4, vec4) -> vec4)
├── fn-refract.glsl          (refract(vec4, vec4, float) -> vec4)
├── fn-min.glsl              (min(vec4, vec4) -> vec4, component-wise)
├── fn-max.glsl              (max(vec4, vec4) -> vec4, component-wise)
├── fn-clamp.glsl            (clamp(vec4, vec4, vec4) -> vec4, component-wise)
├── fn-mix.glsl              (mix(vec4, vec4, vec4) -> vec4, component-wise)
├── fn-step.glsl             (step(vec4, vec4) -> vec4, component-wise)
├── fn-smoothstep.glsl       (smoothstep(vec4, vec4, vec4) -> vec4, component-wise)
├── fn-abs.glsl              (abs(vec4) -> vec4, component-wise)
├── from-scalar.glsl         (vec4(float) - broadcast)
├── from-scalars.glsl        (vec4(float, float, float, float))
├── from-vectors.glsl        (vec4(vec2, vec2), vec4(vec3, float), vec4(float, vec3), vec4(vec4) - combinations)
├── from-shortening.glsl     (vec4(vec4) - identity, no shortening from vec4)
├── from-vec.glsl            (vec4(vec4) - identity)
├── from-mixed.glsl          (vec4(int, int, int, int), vec4(bool, bool, bool, bool) - conversions)
├── to-float.glsl            (float(vec4) - extract first component)
├── to-int.glsl              (int(vec4) - extract first component)
├── to-uint.glsl             (uint(vec4) - extract first component)
├── to-bool.glsl             (bool(vec4) - extract first component)
├── to-ivec.glsl             (ivec4(vec4) - component-wise conversion)
├── to-uvec.glsl             (uvec4(vec4) - component-wise conversion)
├── assign-simple.glsl       (vec4 = vec4)
├── assign-compound.glsl     (vec4 += vec4, vec4 -= vec4, vec4 *= vec4, vec4 /= vec4)
├── assign-element.glsl      (vec4.x = float, vec4[0] = float - single component)
├── assign-swizzle.glsl      (vec4.xy = vec2, vec4.xyz = vec3, vec4.xyzw = vec4 - multi-component swizzle)
├── access-array.glsl        (vec4[0], vec4[1], vec4[2], vec4[3])
├── access-component.glsl    (vec4.x, vec4.y, vec4.z, vec4.w)
├── access-swizzle.glsl      (vec4.xy, vec4.xyzw, vec4.wzyx, vec4.xxxx, etc.)
├── ctrl-if.glsl             (if (any(bvec_from_comparison)) - control flow)
├── ctrl-while.glsl          (while (any(bvec_from_comparison)))
├── ctrl-for.glsl            (for (init; any(bvec_from_comparison); update))
├── ctrl-do-while.glsl       (do { } while (any(bvec_from_comparison)))
├── ctrl-ternary.glsl        (any(bvec_from_comparison) ? expr1 : expr2)
├── edge-zero.glsl           (vec4(0.0, 0.0, 0.0, 0.0) patterns)
├── edge-nan-inf.glsl        (NaN, Inf components)
├── edge-precision.glsl      (floating-point precision)
├── edge-unit-vectors.glsl   (unit vector patterns)
├── edge-large-values.glsl   (large floating-point values)
├── edge-small-values.glsl   (small floating-point values)
└── edge-mixed-components.glsl (mixed component patterns)
```

## Test File Patterns

Each test file should follow the pattern from `vec2/`, `vec3/`, and `bvec2/` tests:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

vec4 test_vec_operation_name() {
    // Test implementation
    return result;
    // Should be vec4(expected_x, expected_y, expected_z, expected_w)
}

// run: test_vec_operation_name() ~= vec4(expected_x, expected_y, expected_z, expected_w)
```

Note: Use `~=` for approximate equality due to floating-point precision issues.

## Key Test Categories

### 1. Arithmetic Operators

**op-add.glsl**: Test `+` operator (component-wise)
- `vec4 + vec4` → `vec4` (component-wise addition)
- Test with positive/negative numbers, NaN, Inf

**op-subtract.glsl**: Test `-` operator (component-wise)
- `vec4 - vec4` → `vec4` (component-wise subtraction)
- Test with positive/negative numbers

**op-multiply.glsl**: Test `*` operator (component-wise)
- `vec4 * vec4` → `vec4` (component-wise multiplication)
- Test with positive/negative numbers, zero, Inf

**op-divide.glsl**: Test `/` operator (component-wise)
- `vec4 / vec4` → `vec4` (component-wise division)
- Test division by zero (produces Inf)
- Test division involving NaN/Inf

### 2. Comparison Operators

**op-equal.glsl**: Test `==` operator (aggregate comparison)
- `vec4 == vec4` → `bool` (true if all components equal)
- Test with matching vectors, partially matching, completely different

**op-not-equal.glsl**: Test `!=` operator (aggregate comparison)
- `vec4 != vec4` → `bool` (true if any component differs)

### 3. Unary Operators

**op-unary-minus.glsl**: Test `-` unary operator (component-wise)
- `-vec4` → `vec4` (component-wise negation)
- Test with positive/negative values, NaN, Inf

### 4. Increment/Decrement Operators

**op-increment-pre.glsl**: Test `++` pre-increment
- `++vec4` → `vec4` (increment all components, return new value)
- Must be on lvalue

**op-increment-post.glsl**: Test `++` post-increment
- `vec4++` → `vec4` (return old value, then increment all components)
- Must be on lvalue

**op-decrement-pre.glsl**: Test `--` pre-decrement
- `--vec4` → `vec4` (decrement all components, return new value)
- Must be on lvalue

**op-decrement-post.glsl**: Test `--` post-decrement
- `vec4--` → `vec4` (return old value, then decrement all components)
- Must be on lvalue

### 5. Built-in Functions

**fn-less-than.glsl**: Test `lessThan()` built-in
- `lessThan(vec4, vec4)` → `bvec4` (component-wise comparison)

**fn-greater-than.glsl**: Test `greaterThan()` built-in
- `greaterThan(vec4, vec4)` → `bvec4` (component-wise comparison)

**fn-less-equal.glsl**: Test `lessThanEqual()` built-in
- `lessThanEqual(vec4, vec4)` → `bvec4` (component-wise comparison)

**fn-greater-equal.glsl**: Test `greaterThanEqual()` built-in
- `greaterThanEqual(vec4, vec4)` → `bvec4` (component-wise comparison)

**fn-equal.glsl**: Test `equal()` built-in
- `equal(vec4, vec4)` → `bvec4` (component-wise equality)

**fn-not-equal.glsl**: Test `notEqual()` built-in
- `notEqual(vec4, vec4)` → `bvec4` (component-wise inequality)

**fn-length.glsl**: Test `length()` built-in
- `length(vec4)` → `float` (Euclidean length)
- Test with unit vectors, zero vectors, various magnitudes

**fn-distance.glsl**: Test `distance()` built-in
- `distance(vec4, vec4)` → `float` (distance between points)
- Test with various point pairs

**fn-dot.glsl**: Test `dot()` built-in
- `dot(vec4, vec4)` → `float` (dot product)
- Test with orthogonal vectors, parallel vectors, various angles

**fn-normalize.glsl**: Test `normalize()` built-in
- `normalize(vec4)` → `vec4` (normalize to unit length)
- Test with various vectors, zero vector (undefined behavior)

**fn-faceforward.glsl**: Test `faceforward()` built-in
- `faceforward(vec4, vec4, vec4)` → `vec4` (flip normal if needed)
- Returns `N` if `dot(I, Nref) < 0`, otherwise `-N`

**fn-reflect.glsl**: Test `reflect()` built-in
- `reflect(vec4, vec4)` → `vec4` (reflection vector)
- Test with various incident and normal vectors

**fn-refract.glsl**: Test `refract()` built-in
- `refract(vec4, vec4, float)` → `vec4` (refraction vector)
- Test with various incident and normal vectors, indices of refraction

**fn-min.glsl**: Test `min()` built-in
- `min(vec4, vec4)` → `vec4` (component-wise minimum)

**fn-max.glsl**: Test `max()` built-in
- `max(vec4, vec4)` → `vec4` (component-wise maximum)

**fn-clamp.glsl**: Test `clamp()` built-in
- `clamp(vec4, vec4, vec4)` → `vec4` (component-wise clamp)

**fn-mix.glsl**: Test `mix()` built-in
- `mix(vec4, vec4, vec4)` → `vec4` (component-wise linear interpolation)

**fn-step.glsl**: Test `step()` built-in
- `step(vec4, vec4)` → `vec4` (component-wise step function)

**fn-smoothstep.glsl**: Test `smoothstep()` built-in
- `smoothstep(vec4, vec4, vec4)` → `vec4` (component-wise smooth step)

**fn-abs.glsl**: Test `abs()` built-in
- `abs(vec4)` → `vec4` (component-wise absolute value)

### 6. Constructors

**from-scalar.glsl**: Test scalar broadcast constructor
- `vec4(float)` - broadcast single float to all components
- `vec4(5.0)` → `vec4(5.0, 5.0, 5.0, 5.0)`

**from-scalars.glsl**: Test constructors from multiple scalars
- `vec4(float, float, float, float)` - from 4 floats
- Various combinations of values

**from-vectors.glsl**: Test constructors from vector combinations
- `vec4(vec2, vec2)` - combine two vec2s
- `vec4(vec3, float)` - combine vec3 and float
- `vec4(float, vec3)` - combine float and vec3
- `vec4(vec4)` - identity constructor
- Test all valid combinations

**from-shortening.glsl**: Test identity constructor (no shortening from vec4)
- `vec4(vec4)` - identity constructor
- Should preserve all components
- Note: vec4 is the largest float vector, so no shortening constructors exist

**from-vec.glsl**: Test identity constructor (alias for from-vectors.glsl)

**from-mixed.glsl**: Test constructors with type conversions
- `vec4(int, int, int, int)` - converts to float
- `vec4(bool, bool, bool, bool)` - converts to float (false → 0.0, true → 1.0)
- Test various numeric inputs

### 7. Conversions

**to-float.glsl**: Test conversion to scalar float
- `float(vec4)` - extracts first component
- `float(vec4(5.0, 10.0, 15.0, 20.0))` → `5.0` (first component)

**to-int.glsl**: Test conversion to scalar int
- `int(vec4)` - converts first component (truncates)
- Test with positive/negative floats, NaN, Inf

**to-uint.glsl**: Test conversion to scalar uint
- `uint(vec4)` - converts first component (truncates)
- Test with positive floats, NaN, Inf, negative values

**to-bool.glsl**: Test conversion to scalar bool
- `bool(vec4)` - converts first component (0.0 → false, non-zero → true)
- Test with NaN (NaN → true)

**to-ivec.glsl**: Test conversion to ivec4
- `ivec4(vec4)` - component-wise conversion (truncates)
- Test with positive/negative floats, NaN, Inf

**to-uvec.glsl**: Test conversion to uvec4
- `uvec4(vec4)` - component-wise conversion (truncates)
- Test with positive floats, NaN, Inf, negative values

### 8. Assignment

**assign-simple.glsl**: Test simple assignment
- `vec4 a = vec4(...); vec4 b = a;` - assignment
- Verify independence (modifying one doesn't affect the other)
- Self-assignment: `vec4 = vec4`

**assign-compound.glsl**: Test compound assignment operators
- `vec4 += vec4` - add and assign
- `vec4 -= vec4` - subtract and assign
- `vec4 *= vec4` - multiply and assign
- `vec4 /= vec4` - divide and assign
- Test component-wise behavior

**assign-element.glsl**: Test single component assignment
- `vec4.x = float` - assign to single component by name
- `vec4[0] = float` - assign to single component by index
- Verify other components unchanged
- Test all components: x, y, z, w and indices 0, 1, 2, 3

**assign-swizzle.glsl**: Test multi-component swizzle assignment
- `vec4.xy = vec2(...)` - assign to swizzle
- `vec4.xyz = vec3(...)` - assign to swizzle
- `vec4.xyzw = vec4(...)` - assign to swizzle
- Verify components are updated correctly
- Test various swizzle patterns (no duplicates allowed in assignment)

### 9. Component Access

**access-array.glsl**: Test array-style indexing
- `vec4[0]`, `vec4[1]`, `vec4[2]`, `vec4[3]` - array indexing
- Variable indexing: `vec4[i]` where `i` is computed
- Verify correct component access

**access-component.glsl**: Test component name access
- `vec4.x`, `vec4.y`, `vec4.z`, `vec4.w` - component access
- Verify correct component values

**access-swizzle.glsl**: Test component swizzling
- `vec4.xy` → `vec2`
- `vec4.xyz` → `vec3`
- `vec4.xyzw` → `vec4` (identity)
- `vec4.wzyx` → `vec4` (reverse)
- `vec4.xxxx` → `vec4` (duplicate)
- Test all name sets: `xyzw`, `rgba`, `stpq`
- Test various patterns: `xy`, `xyz`, `xyzw`, `wzyx`, `xxxx`, etc.

### 10. Control Flow

**ctrl-if.glsl**: Test `if` statements with vec4
- Control flow conditions must be scalar `bool`, so use built-ins that return `bvec4`, then `any()` or `all()`
- `if (any(lessThan(vec4, vec4)))` - condition using any()
- `if (all(equal(vec4, vec4)))` - condition using all()

**ctrl-while.glsl**: Test `while` loops with vec4
- `while (any(greaterThan(vec4, vec4)))` - loop condition

**ctrl-for.glsl**: Test `for` loops with vec4
- `for (init; any(notEqual(vec4, vec4)); update)` - for loop condition

**ctrl-do-while.glsl**: Test `do-while` loops with vec4
- `do { } while (any(lessThan(vec4, vec4)))` - do-while condition

**ctrl-ternary.glsl**: Test ternary operator with vec4
- `any(equal(vec4, vec4)) ? expr1 : expr2` - ternary with vec4 condition

### 11. Edge Cases

**edge-zero.glsl**: Test edge case with all components zero
- `vec4(0.0, 0.0, 0.0, 0.0)` patterns
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

**edge-large-values.glsl**: Test large floating-point values
- Very large numbers
- Operations that might overflow

**edge-small-values.glsl**: Test small floating-point values
- Very small numbers
- Operations that might underflow

**edge-mixed-components.glsl**: Test various mixed component patterns
- Different patterns: `(1.0, -1.0, 5.0, 0.5)`, `(100.0, 0.001, -50.0, 1e-10)`, etc.
- Verify component-wise operations work correctly

## Implementation Notes

1. **Test Format**: Follow the exact format from `vec2/`, `vec3/`, and `bvec2/` tests with:
   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results (use `~=` for approximate equality)
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:
   - All operators from GLSL spec (operators.adoc)
   - All constructor forms (operators.adoc, constructors section)
   - All conversion forms (operators.adoc, conversion section)
   - All built-in functions (builtinfunctions.adoc: GenFType functions, geometric functions)
   - Component access (swizzling, indexing)
   - Control flow requirements (statements.adoc: conditions must be bool, use any/all for bvec)

3. **Key Differences from vec2/vec3**:
   - Additional components (w) and index [3]
   - More vector combination constructors: `vec4(vec2, vec2)`, `vec4(vec3, float)`, `vec4(float, vec3)`
   - Extended swizzle patterns including w component
   - Additional geometric functions: `faceforward()`, `reflect()`, `refract()`
   - No shortening constructors (vec4 is largest float vector)

4. **Expected Failures**: These tests are expected to fail initially, especially:
   - Built-in functions (geometric including faceforward/reflect/refract, relational, common)
   - Some constructor forms (vector combinations, mixed types)
   - Some conversion forms
   - Swizzle assignment
   - Increment/decrement operators
   - Compound assignment operators

5. **Migration from Existing Tests**: The existing nested `vec4/` directory structure should be migrated to this flat structure:
   - `vec4/arithmetic/*` → `vec4/op-*.glsl`
   - `vec4/assignment/*` → `vec4/assign-*.glsl`
   - `vec4/builtins/*` → `vec4/fn-*.glsl`
   - `vec4/constructors/*` → `vec4/from-*.glsl`
   - `vec4/indexing/*` → `vec4/access-*.glsl`
   - `vec4/relational/*` → `vec4/fn-*.glsl` and `vec4/op-*.glsl`
   - `vec4/increment-decrement/*` → `vec4/op-*-*.glsl`
   - `vec4/edge-cases/*` → `vec4/edge-*.glsl`

## Files to Create

Create 50 test files in the flat `vec4/` directory structure above, with each file containing 3-10 test functions following the vec2/vec3/bvec2 pattern. All files use the prefix naming convention:

- `op-*` for operators (arithmetic, comparison, unary, increment/decrement)
- `fn-*` for built-in functions (comparison, geometric including faceforward/reflect/refract, min/max, interpolation)
- `from-*` for constructors (from-scalar, from-scalars, from-vectors, from-shortening, from-vec, from-mixed)
- `to-*` for conversions
- `assign-*` for assignments (assign-simple, assign-compound, assign-element, assign-swizzle)
- `access-*` for component access (array, component, swizzle)
- `ctrl-*` for control flow
- `edge-*` for edge cases

## GLSL Spec References

- **operators.adoc**: Constructors (lines 171-229), Vector components (lines 427-579), Arithmetic operators (lines 580-700), Conversions (lines 908-1100)
- **builtinfunctions.adoc**: Geometric functions (lines 1313-1450), Relational functions (lines 1228-1312), Common functions for `GenFType`

