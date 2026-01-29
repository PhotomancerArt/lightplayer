# Plan: Create Comprehensive bvec4 Tests

## Overview

Create a complete test suite for boolean vector type `bvec4` in `lightplayer/crates/lp-glsl-filetests/filetests/bvec4/` following the organizational pattern used in `vec4/` and `bool/`. These tests will comprehensively cover the GLSL boolean vector specification for `bvec4` and are expected to fail initially, serving as a specification for implementing the rest of the compiler.

## Directory Structure

Following the flat naming convention with prefixes (like `vec4/` and `bool/`), create tests in a single `bvec4/` directory:

```javascript
bvec4/
├── op-equal.glsl              (bvec4 == bvec4 -> bool, equal(bvec4, bvec4) -> bvec4)
├── op-not-equal.glsl         (bvec4 != bvec4 -> bool, notEqual(bvec4, bvec4) -> bvec4)
├── op-not.glsl                (not(bvec4) -> bvec4 - component-wise NOT)
├── fn-any.glsl                (any(bvec4) -> bool)
├── fn-all.glsl                (all(bvec4) -> bool)
├── fn-mix.glsl                (mix(bvec4, bvec4, bvec4) -> bvec4)
├── from-scalar.glsl           (bvec4(bool) - broadcast)
├── from-scalars.glsl          (bvec4(bool, bool, bool, bool))
├── from-vectors.glsl          (bvec4(bvec2, bvec2), bvec4(bvec3, bool), etc.)
├── from-bvec.glsl             (bvec4(bvec4) - identity)
├── from-mixed.glsl            (bvec4(int, int, float, float) - type conversions)
├── to-bool.glsl                (bool(bvec4) - extract first component)
├── to-int.glsl                 (int(bvec4) - extract first component)
├── to-uint.glsl                (uint(bvec4) - extract first component)
├── to-float.glsl               (float(bvec4) - extract first component)
├── to-ivec.glsl                (ivec4(bvec4) - component-wise conversion)
├── to-uvec.glsl                (uvec4(bvec4) - component-wise conversion)
├── to-vec.glsl                 (vec4(bvec4) - component-wise conversion)
├── assign-simple.glsl          (bvec4 = bvec4)
├── assign-element.glsl         (bvec4.x = bool, bvec4[0] = bool - single component)
├── assign-swizzle.glsl         (bvec4.xy = bvec2 - multi-component swizzle)
├── access-array.glsl           (bvec4[0], bvec4[1], bvec4[2], bvec4[3])
├── access-component.glsl       (bvec4.x, bvec4.y, bvec4.z, bvec4.w)
├── access-swizzle.glsl         (bvec4.xy, bvec4.xyz, bvec4.xyzw, bvec4.wzyx, etc.)
├── ctrl-if.glsl                (if (any(bvec4)), if (all(bvec4)))
├── ctrl-while.glsl              (while (any(bvec4)))
├── ctrl-for.glsl                (for (init; any(bvec4); update))
├── ctrl-do-while.glsl           (do { } while (any(bvec4)))
├── ctrl-ternary.glsl            (? : operator with bvec4 condition via any/all)
├── edge-nested.glsl             (nested bvec4 operations)
├── edge-mixed-components.glsl  (mixed true/false patterns)
├── edge-all-true.glsl          (bvec4(true, true, true, true) patterns)
└── edge-all-false.glsl         (bvec4(false, false, false, false) patterns)
```

## Test File Patterns

Each test file should follow the pattern from `vec4/` and `bool/` tests:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

bvec4 test_bvec_operation_name() {
    // Test implementation
    return result;
    // Should be bvec4(true, false, true, false)
}

// run: test_bvec_operation_name() == bvec4(true, false, true, false)
```

## Key Test Categories

### 1. Comparison Operators

**op-equal.glsl**: Test `==` operator and `equal()` built-in

- `bvec4 == bvec4` → `bool` (aggregate comparison - all components must match)
- `equal(bvec4, bvec4)` → `bvec4` (component-wise comparison)
- Test with matching vectors, partially matching, completely different

**op-not-equal.glsl**: Test `!=` operator and `notEqual()` built-in

- `bvec4 != bvec4` → `bool` (aggregate comparison)
- `notEqual(bvec4, bvec4)` → `bvec4` (component-wise comparison)

### 2. Logical Operations

**op-not.glsl**: Test `not()` built-in (component-wise logical NOT)

- `not(bvec4(true, false, true, false))` → `bvec4(false, true, false, true)`
- Double negation: `not(not(bvec4))` should equal original
- Note: `!` operator works on scalar `bool` only, NOT on `bvec4` (per GLSL spec)

### 3. Built-in Functions

**fn-any.glsl**: Test `any()` function

- `any(bvec4)` → `bool` (true if any component is true)
- `any(bvec4(true, false, false, false))` → `true`
- `any(bvec4(false, false, false, false))` → `false`
- `any(bvec4(true, true, true, true))` → `true` (all true case)

**fn-all.glsl**: Test `all()` function

- `all(bvec4)` → `bool` (true only if all components are true)
- `all(bvec4(true, true, true, true))` → `true`
- `all(bvec4(true, false, true, true))` → `false`
- `all(bvec4(false, false, false, false))` → `false` (all false case)

**fn-mix.glsl**: Test `mix()` function with `bvec4`

- `mix(bvec4, bvec4, bvec4)` → `bvec4` (component-wise selection)
- For each component: if selector is `false`, take from first arg; if `true`, take from second arg
- `mix(bvec4(true, true, false, false), bvec4(false, false, true, true), bvec4(false, true, false, true))` → `bvec4(true, false, false, true)`

### 4. Constructors

**from-scalar.glsl**: Test scalar broadcast constructor

- `bvec4(bool)` - broadcast single bool to all components
- `bvec4(true)` → `bvec4(true, true, true, true)`
- `bvec4(false)` → `bvec4(false, false, false, false)`

**from-scalars.glsl**: Test constructors from multiple scalar bools

- `bvec4(bool, bool, bool, bool)` - from 4 bools
- Various combinations of `true`/`false`

**from-vectors.glsl**: Test constructors from vectors

- `bvec4(bvec2, bvec2)` - combine two bvec2s
- `bvec4(bvec3, bool)` - combine bvec3 and bool
- `bvec4(bool, bvec3)` - combine bool and bvec3
- `bvec4(bvec2, bool, bool)` - mixed combinations
- `bvec4(bool, bvec2, bool)` - mixed combinations
- `bvec4(bool, bool, bvec2)` - mixed combinations
- Test all valid combinations for constructing bvec4

**from-bvec.glsl**: Test identity constructor

- `bvec4(bvec4)` - identity constructor
- Should preserve all components

**from-mixed.glsl**: Test constructors with type conversions

- `bvec4(int, int, float, float)` - converts to bool (0/0.0 → false, non-zero → true)
- `bvec4(0, 1, 0.0, 1.0)` → `bvec4(false, true, false, true)`
- Test various numeric inputs

### 5. Conversions

**to-bool.glsl**: Test conversion to scalar bool

- `bool(bvec4)` - extracts first component
- `bool(bvec4(true, false, true, false))` → `true` (first component)

**to-int.glsl**: Test conversion to scalar int

- `int(bvec4)` - converts first component (false → 0, true → 1)

**to-uint.glsl**: Test conversion to scalar uint

- `uint(bvec4)` - converts first component (false → 0u, true → 1u)

**to-float.glsl**: Test conversion to scalar float

- `float(bvec4)` - converts first component (false → 0.0, true → 1.0)

**to-ivec.glsl**: Test conversion to ivec4

- `ivec4(bvec4)` - component-wise conversion
- `ivec4(bvec4(true, false, true, false))` → `ivec4(1, 0, 1, 0)`

**to-uvec.glsl**: Test conversion to uvec4

- `uvec4(bvec4)` - component-wise conversion
- `uvec4(bvec4(true, false, true, false))` → `uvec4(1u, 0u, 1u, 0u)`

**to-vec.glsl**: Test conversion to vec4

- `vec4(bvec4)` - component-wise conversion
- `vec4(bvec4(true, false, true, false))` → `vec4(1.0, 0.0, 1.0, 0.0)`

### 6. Assignment

**assign-simple.glsl**: Test simple assignment

- `bvec4 a = bvec4(...); bvec4 b = a;` - assignment
- Verify independence (modifying one doesn't affect the other)
- Self-assignment: `bvec = bvec`

**assign-element.glsl**: Test single component assignment

- `bvec4.x = bool` - assign to single component by name
- `bvec4[0] = bool` - assign to single component by index
- Verify other components unchanged
- Test all components: x, y, z, w and indices 0, 1, 2, 3

**assign-swizzle.glsl**: Test multi-component swizzle assignment

- `bvec4.xy = bvec2(...)` - assign to swizzle
- `bvec4.xyz = bvec3(...)` - assign to swizzle
- Verify components are updated correctly
- Test various swizzle patterns (no duplicates allowed in assignment)

### 7. Component Access

**access-array.glsl**: Test array-style indexing

- `bvec4[0]`, `bvec4[1]`, `bvec4[2]`, `bvec4[3]` - array indexing
- Variable indexing: `bvec4[i] `where `i` is computed
- Verify correct component access

**access-component.glsl**: Test component name access

- `bvec4.x`, `bvec4.y`, `bvec4.z`, `bvec4.w` - component access
- Verify correct component values

**access-swizzle.glsl**: Test component swizzling

- `bvec4.xy` → `bvec2`
- `bvec4.xyz` → `bvec3`
- `bvec4.xyzw` → `bvec4` (identity)
- `bvec4.wzyx` → `bvec4` (reverse)
- `bvec4.xxyy` → `bvec4` (duplicate)
- Test all name sets: `xyzw`, `rgba`, `stpq`
- Test various patterns: `xy`, `zw`, `xz`, `wzyx`, etc.

### 8. Control Flow

**ctrl-if.glsl**: Test `if` statements with bvec4

- `if (any(bvec4))` - condition using any()
- `if (all(bvec4))` - condition using all()
- Note: Control flow conditions must be scalar `bool`, so use `any()` or `all()` to convert

**ctrl-while.glsl**: Test `while` loops with bvec4

- `while (any(bvec4))` - loop condition
- `while (all(bvec4))` - loop condition

**ctrl-for.glsl**: Test `for` loops with bvec4

- `for (init; any(bvec4); update)` - for loop condition

**ctrl-do-while.glsl**: Test `do-while` loops with bvec4

- `do { } while (any(bvec4))` - do-while condition

**ctrl-ternary.glsl**: Test ternary operator with bvec4

- `any(bvec4) ? expr1 : expr2` - ternary with bvec4 condition
- `all(bvec4) ? expr1 : expr2` - ternary with bvec4 condition

### 9. Edge Cases

**edge-nested.glsl**: Test nested bvec4 operations

- `not(equal(bvec4(...), bvec4(...)))`
- `any(not(bvec4(...)))`
- `all(equal(bvec4(...), bvec4(...)))`
- Complex nested expressions

**edge-mixed-components.glsl**: Test various mixed true/false patterns

- Different patterns: `(true, false, true, false)`, `(false, true, false, true)`, etc.
- Verify component-wise operations work correctly

**edge-all-true.glsl**: Test edge case with all components true

- `bvec4(true, true, true, true)` patterns
- Test operations on all-true vectors
- Verify `all()` returns true, `any()` returns true

**edge-all-false.glsl**: Test edge case with all components false

- `bvec4(false, false, false, false)` patterns
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

- Logical operators (`&&`, `||`, `^^`, `!`) work on scalar `bool` only, NOT on `bvec4`
- Use `not(bvec4)` built-in instead of `!bvec4`
- Use `any(bvec4)` or `all(bvec4)` to convert `bvec4` to `bool` for control flow
- `==` and `!=` operators return `bool` (aggregate comparison)
- Use `equal()` and `notEqual()` built-ins for component-wise comparison returning `bvec4`

4. **Expected Failures**: These tests are expected to fail initially, especially:

- `not()` built-in function
- `any()` and `all()` built-in functions
- `mix()` with `bvec` arguments
- Some constructor forms (mixed types)
- Some conversion forms
- Swizzle assignment

5. **Reference Files**:

- Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/relational/equal.glsl`
- Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/relational/not.glsl`
- Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/assignment/simple-assignment.glsl`
- Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/indexing/swizzling.glsl` (for access-swizzle.glsl)
- Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/vec4/assignment/element-assignment.glsl` (for assign-element.glsl)

## Files to Create

Create 33 test files in the flat `bvec4/` directory structure above, with each file containing 3-10 test functions following the vec4/bool pattern. All files use the prefix naming convention:

- `op-*` for operators (comparison)
- `fn-*` for built-in functions (fn-any, fn-all, fn-mix)
- `from-*` for constructors (from-scalar, from-scalars, from-vectors, from-bvec, from-mixed)
- `to-*` for conversions
- `assign-*` for assignments (assign-simple, assign-element, assign-swizzle)
- `access-*` for component access (array, component, swizzle)
- `ctrl-*` for control flow
- `edge-*` for edge cases

## GLSL Spec References

- **operators.adoc**: Constructors (lines 171-229), Vector components (lines 427-579), Equality operators (lines 885-907)
- **builtinfunctions.adoc**: Relational functions (lines 1228-1312), `any()`, `all()`, `not()`, `equal()`, `notEqual()`, `mix()` with `bvec`
