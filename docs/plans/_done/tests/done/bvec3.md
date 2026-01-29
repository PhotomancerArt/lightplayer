# Plan: Create Comprehensive bvec3 Tests

## Overview

Create a complete test suite for boolean vector type `bvec3` in `lightplayer/crates/lp-glsl-filetests/filetests/bvec3/` following the organizational pattern used in `vec4/` and `bool/`. These tests will comprehensively cover the GLSL boolean vector specification for `bvec3` and are expected to fail initially, serving as a specification for implementing the rest of the compiler.

## Directory Structure

Following the flat naming convention with prefixes (like `vec4/` and `bool/`), create tests in a single `bvec3/` directory:

```javascript
bvec3/
├── op-equal.glsl              (bvec3 == bvec3 -> bool, equal(bvec3, bvec3) -> bvec3)
├── op-not-equal.glsl         (bvec3 != bvec3 -> bool, notEqual(bvec3, bvec3) -> bvec3)
├── op-not.glsl                (not(bvec3) -> bvec3 - component-wise NOT)
├── fn-any.glsl                (any(bvec3) -> bool)
├── fn-all.glsl                (all(bvec3) -> bool)
├── fn-mix.glsl                (mix(bvec3, bvec3, bvec3) -> bvec3)
├── from-scalar.glsl           (bvec3(bool) - broadcast)
├── from-scalars.glsl          (bvec3(bool, bool, bool))
├── from-vectors.glsl          (bvec3(bvec2, bool), bvec3(bool, bvec2), etc.)
├── from-shortening.glsl       (bvec3(bvec4) - shortening constructor)
├── from-bvec.glsl             (bvec3(bvec3) - identity)
├── from-mixed.glsl            (bvec3(int, int, float) - type conversions)
├── to-bool.glsl                (bool(bvec3) - extract first component)
├── to-int.glsl                 (int(bvec3) - extract first component)
├── to-uint.glsl                (uint(bvec3) - extract first component)
├── to-float.glsl               (float(bvec3) - extract first component)
├── to-ivec.glsl                (ivec3(bvec3) - component-wise conversion)
├── to-uvec.glsl                (uvec3(bvec3) - component-wise conversion)
├── to-vec.glsl                 (vec3(bvec3) - component-wise conversion)
├── assign-simple.glsl          (bvec3 = bvec3)
├── assign-element.glsl         (bvec3.x = bool, bvec3[0] = bool - single component)
├── assign-swizzle.glsl         (bvec3.xy = bvec2 - multi-component swizzle)
├── access-array.glsl           (bvec3[0], bvec3[1], bvec3[2])
├── access-component.glsl       (bvec3.x, bvec3.y, bvec3.z)
├── access-swizzle.glsl         (bvec3.xy, bvec3.xyz, bvec3.zyx, etc.)
├── ctrl-if.glsl                (if (any(bvec3)), if (all(bvec3)))
├── ctrl-while.glsl              (while (any(bvec3)))
├── ctrl-for.glsl                (for (init; any(bvec3); update))
├── ctrl-do-while.glsl           (do { } while (any(bvec3)))
├── ctrl-ternary.glsl            (? : operator with bvec3 condition via any/all)
├── edge-nested.glsl             (nested bvec3 operations)
├── edge-mixed-components.glsl  (mixed true/false patterns)
├── edge-all-true.glsl          (bvec3(true, true, true) patterns)
└── edge-all-false.glsl         (bvec3(false, false, false) patterns)
```

## Test File Patterns

Each test file should follow the pattern from `vec4/` and `bool/` tests:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

bvec3 test_bvec_operation_name() {
    // Test implementation
    return result;
    // Should be bvec3(true, false, true)
}

// run: test_bvec_operation_name() == bvec3(true, false, true)
```

## Key Test Categories

### 1. Comparison Operators

**op-equal.glsl**: Test `==` operator and `equal()` built-in

- `bvec3 == bvec3` → `bool` (aggregate comparison - all components must match)
- `equal(bvec3, bvec3)` → `bvec3` (component-wise comparison)
- Test with matching vectors, partially matching, completely different

**op-not-equal.glsl**: Test `!=` operator and `notEqual()` built-in

- `bvec3 != bvec3` → `bool` (aggregate comparison)
- `notEqual(bvec3, bvec3)` → `bvec3` (component-wise comparison)

### 2. Logical Operations

**op-not.glsl**: Test `not()` built-in (component-wise logical NOT)

- `not(bvec3(true, false, true))` → `bvec3(false, true, false)`
- Double negation: `not(not(bvec3))` should equal original
- Note: `!` operator works on scalar `bool` only, NOT on `bvec3` (per GLSL spec)

### 3. Built-in Functions

**fn-any.glsl**: Test `any()` function

- `any(bvec3)` → `bool` (true if any component is true)
- `any(bvec3(true, false, false))` → `true`
- `any(bvec3(false, false, false))` → `false`
- `any(bvec3(true, true, true))` → `true` (all true case)

**fn-all.glsl**: Test `all()` function

- `all(bvec3)` → `bool` (true only if all components are true)
- `all(bvec3(true, true, true))` → `true`
- `all(bvec3(true, false, true))` → `false`
- `all(bvec3(false, false, false))` → `false` (all false case)

**fn-mix.glsl**: Test `mix()` function with `bvec3`

- `mix(bvec3, bvec3, bvec3)` → `bvec3` (component-wise selection)
- For each component: if selector is `false`, take from first arg; if `true`, take from second arg
- `mix(bvec3(true, true, false), bvec3(false, false, true), bvec3(false, true, false))` → `bvec3(true, false, false)`

### 4. Constructors

**from-scalar.glsl**: Test scalar broadcast constructor

- `bvec3(bool)` - broadcast single bool to all components
- `bvec3(true)` → `bvec3(true, true, true)`
- `bvec3(false)` → `bvec3(false, false, false)`

**from-scalars.glsl**: Test constructors from multiple scalar bools

- `bvec3(bool, bool, bool)` - from 3 bools
- Various combinations of `true`/`false`

**from-vectors.glsl**: Test constructors from vectors

- `bvec3(bvec2, bool)` - combine bvec2 and bool
- `bvec3(bool, bvec2)` - combine bool and bvec2
- Test all valid combinations for constructing bvec3

**from-shortening.glsl**: Test shortening constructor

- `bvec3(bvec4)` - extract first three components
- Verify components are preserved in order

**from-bvec.glsl**: Test identity constructor

- `bvec3(bvec3)` - identity constructor
- Should preserve all components

**from-mixed.glsl**: Test constructors with type conversions

- `bvec3(int, int, float)` - converts to bool (0/0.0 → false, non-zero → true)
- `bvec3(0, 1, 0.0)` → `bvec3(false, true, false)`
- Test various numeric inputs

### 5. Conversions

**to-bool.glsl**: Test conversion to scalar bool

- `bool(bvec3)` - extracts first component
- `bool(bvec3(true, false, true))` → `true` (first component)

**to-int.glsl**: Test conversion to scalar int

- `int(bvec3)` - converts first component (false → 0, true → 1)

**to-uint.glsl**: Test conversion to scalar uint

- `uint(bvec3)` - converts first component (false → 0u, true → 1u)

**to-float.glsl**: Test conversion to scalar float

- `float(bvec3)` - converts first component (false → 0.0, true → 1.0)

**to-ivec.glsl**: Test conversion to ivec3

- `ivec3(bvec3)` - component-wise conversion
- `ivec3(bvec3(true, false, true))` → `ivec3(1, 0, 1)`

**to-uvec.glsl**: Test conversion to uvec3

- `uvec3(bvec3)` - component-wise conversion
- `uvec3(bvec3(true, false, true))` → `uvec3(1u, 0u, 1u)`

**to-vec.glsl**: Test conversion to vec3

- `vec3(bvec3)` - component-wise conversion
- `vec3(bvec3(true, false, true))` → `vec3(1.0, 0.0, 1.0)`

### 6. Assignment

**assign-simple.glsl**: Test simple assignment

- `bvec3 a = bvec3(...); bvec3 b = a;` - assignment
- Verify independence (modifying one doesn't affect the other)
- Self-assignment: `bvec = bvec`

**assign-element.glsl**: Test single component assignment

- `bvec3.x = bool` - assign to single component by name
- `bvec3[0] = bool` - assign to single component by index
- Verify other components unchanged
- Test all components: x, y, z and indices 0, 1, 2

**assign-swizzle.glsl**: Test multi-component swizzle assignment

- `bvec3.xy = bvec2(...)` - assign to swizzle
- `bvec3.xyz = bvec3(...)` - assign to swizzle
- Verify components are updated correctly
- Test various swizzle patterns (no duplicates allowed in assignment)

### 7. Component Access

**access-array.glsl**: Test array-style indexing

- `bvec3[0]`, `bvec3[1]`, `bvec3[2]` - array indexing
- Variable indexing: `bvec3[i] `where `i` is computed
- Verify correct component access

**access-component.glsl**: Test component name access

- `bvec3.x`, `bvec3.y`, `bvec3.z` - component access
- Verify correct component values

**access-swizzle.glsl**: Test component swizzling

- `bvec3.xy` → `bvec2`
- `bvec3.xyz` → `bvec3` (identity)
- `bvec3.zyx` → `bvec3` (reverse)
- `bvec3.xxy` → `bvec3` (duplicate)
- Test all name sets: `xyz`, `rgb`, `stp`
- Test various patterns: `xy`, `xz`, `yz`, `zyx`, etc.

### 8. Control Flow

**ctrl-if.glsl**: Test `if` statements with bvec3

- `if (any(bvec3))` - condition using any()
- `if (all(bvec3))` - condition using all()
- Note: Control flow conditions must be scalar `bool`, so use `any()` or `all()` to convert

**ctrl-while.glsl**: Test `while` loops with bvec3

- `while (any(bvec3))` - loop condition
- `while (all(bvec3))` - loop condition

**ctrl-for.glsl**: Test `for` loops with bvec3

- `for (init; any(bvec3); update)` - for loop condition

**ctrl-do-while.glsl**: Test `do-while` loops with bvec3

- `do { } while (any(bvec3))` - do-while condition

**ctrl-ternary.glsl**: Test ternary operator with bvec3

- `any(bvec3) ? expr1 : expr2` - ternary with bvec3 condition
- `all(bvec3) ? expr1 : expr2` - ternary with bvec3 condition

### 9. Edge Cases

**edge-nested.glsl**: Test nested bvec3 operations

- `not(equal(bvec3(...), bvec3(...)))`
- `any(not(bvec3(...)))`
- `all(equal(bvec3(...), bvec3(...)))`
- Complex nested expressions

**edge-mixed-components.glsl**: Test various mixed true/false patterns

- Different patterns: `(true, false, true)`, `(false, true, false)`, etc.
- Verify component-wise operations work correctly

**edge-all-true.glsl**: Test edge case with all components true

- `bvec3(true, true, true)` patterns
- Test operations on all-true vectors
- Verify `all()` returns true, `any()` returns true

**edge-all-false.glsl**: Test edge case with all components false

- `bvec3(false, false, false)` patterns
- Test operations on all-false vectors
- Verify `all()` returns false, `any()` returns false

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
- All built-in functions (builtinfunctions.adoc: any, all, not, equal, notEqual, mix)
- Component access (swizzling, indexing)
- Control flow requirements (statements.adoc: conditions must be bool, use any/all for bvec)

3. **Key Differences from scalar bool**:

- Logical operators (`&&`, `||`, `^^`, `!`) work on scalar `bool` only, NOT on `bvec3`
- Use `not(bvec3)` built-in instead of `!bvec3`
- Use `any(bvec3)` or `all(bvec3)` to convert `bvec3` to `bool` for control flow
- `==` and `!=` operators return `bool` (aggregate comparison)
- Use `equal()` and `notEqual()` built-ins for component-wise comparison returning `bvec3`

4. **Expected Failures**: These tests are expected to fail initially, especially:

- `not()` built-in function
- `any()` and `all()` built-in functions
- `mix()` with `bvec` arguments
- Some constructor forms (shortening constructors, mixed types)
- Some conversion forms
- Swizzle assignment

5. **Reference Files**:

- Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/relational/equal.glsl`
- Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/relational/not.glsl`
- Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/assignment/simple-assignment.glsl`
- Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/indexing/swizzling.glsl` (for access-swizzle.glsl)
- Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/assignment/element-assignment.glsl` (for assign-element.glsl)
- Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/constructors/shortening.glsl` (for from-shortening.glsl)

## Files to Create

Create 33 test files in the flat `bvec3/` directory structure above, with each file containing 3-10 test functions following the vec4/bool pattern. All files use the prefix naming convention:

- `op-*` for operators (comparison)
- `fn-*` for built-in functions (fn-any, fn-all, fn-mix)
- `from-*` for constructors (from-scalar, from-scalars, from-vectors, from-shortening, from-bvec, from-mixed)
- `to-*` for conversions
- `assign-*` for assignments (assign-simple, assign-element, assign-swizzle)
- `access-*` for component access (array, component, swizzle)
- `ctrl-*` for control flow
- `edge-*` for edge cases

## GLSL Spec References

- **operators.adoc**: Constructors (lines 171-229), Vector components (lines 427-579), Equality operators (lines 885-907)
- **builtinfunctions.adoc**: Relational functions (lines 1228-1312), `any()`, `all()`, `not()`, `equal()`, `notEqual()`, `mix()` with `bvec`

