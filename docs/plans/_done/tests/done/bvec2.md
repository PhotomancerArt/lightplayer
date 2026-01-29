# Plan: Create Comprehensive bvec2 Tests

## Overview

Create a complete test suite for boolean vector type `bvec2` in `lightplayer/crates/lp-glsl-filetests/filetests/bvec2/` following the organizational pattern used in `vec4/` and `bool/`. These tests will comprehensively cover the GLSL boolean vector specification for `bvec2` and are expected to fail initially, serving as a specification for implementing the rest of the compiler.

## Directory Structure

Following the flat naming convention with prefixes (like `vec4/` and `bool/`), create tests in a single `bvec2/` directory:

```javascript
bvec2/
├── op-equal.glsl              (bvec2 == bvec2 -> bool, equal(bvec2, bvec2) -> bvec2)
├── op-not-equal.glsl         (bvec2 != bvec2 -> bool, notEqual(bvec2, bvec2) -> bvec2)
├── op-not.glsl                (not(bvec2) -> bvec2 - component-wise NOT)
├── fn-any.glsl                (any(bvec2) -> bool)
├── fn-all.glsl                (all(bvec2) -> bool)
├── fn-mix.glsl                (mix(bvec2, bvec2, bvec2) -> bvec2)
├── from-scalar.glsl           (bvec2(bool) - broadcast)
├── from-scalars.glsl          (bvec2(bool, bool))
├── from-shortening.glsl       (bvec2(bvec3), bvec2(bvec4) - shortening constructors)
├── from-bvec.glsl             (bvec2(bvec2) - identity)
├── from-mixed.glsl            (bvec2(int, float) - type conversions)
├── to-bool.glsl                (bool(bvec2) - extract first component)
├── to-int.glsl                 (int(bvec2) - extract first component)
├── to-uint.glsl                (uint(bvec2) - extract first component)
├── to-float.glsl               (float(bvec2) - extract first component)
├── to-ivec.glsl                (ivec2(bvec2) - component-wise conversion)
├── to-uvec.glsl                (uvec2(bvec2) - component-wise conversion)
├── to-vec.glsl                 (vec2(bvec2) - component-wise conversion)
├── assign-simple.glsl          (bvec2 = bvec2)
├── assign-element.glsl         (bvec2.x = bool, bvec2[0] = bool - single component)
├── assign-swizzle.glsl         (bvec2.xy = bvec2 - multi-component swizzle)
├── access-array.glsl           (bvec2[0], bvec2[1])
├── access-component.glsl       (bvec2.x, bvec2.y)
├── access-swizzle.glsl         (bvec2.xy, bvec2.yx, bvec2.xx, etc.)
├── ctrl-if.glsl                (if (any(bvec2)), if (all(bvec2)))
├── ctrl-while.glsl              (while (any(bvec2)))
├── ctrl-for.glsl                (for (init; any(bvec2); update))
├── ctrl-do-while.glsl           (do { } while (any(bvec2)))
├── ctrl-ternary.glsl            (? : operator with bvec2 condition via any/all)
├── edge-nested.glsl             (nested bvec2 operations)
├── edge-mixed-components.glsl  (mixed true/false patterns)
├── edge-all-true.glsl          (bvec2(true, true) patterns)
└── edge-all-false.glsl         (bvec2(false, false) patterns)
```

## Test File Patterns

Each test file should follow the pattern from `vec4/` and `bool/` tests:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

bvec2 test_bvec_operation_name() {
    // Test implementation
    return result;
    // Should be bvec2(true, false)
}

// run: test_bvec_operation_name() == bvec2(true, false)
```

## Key Test Categories

### 1. Comparison Operators

**op-equal.glsl**: Test `==` operator and `equal()` built-in

- `bvec2 == bvec2` → `bool` (aggregate comparison - all components must match)
- `equal(bvec2, bvec2)` → `bvec2` (component-wise comparison)
- Test with matching vectors, partially matching, completely different

**op-not-equal.glsl**: Test `!=` operator and `notEqual()` built-in

- `bvec2 != bvec2` → `bool` (aggregate comparison)
- `notEqual(bvec2, bvec2)` → `bvec2` (component-wise comparison)

### 2. Logical Operations

**op-not.glsl**: Test `not()` built-in (component-wise logical NOT)

- `not(bvec2(true, false))` → `bvec2(false, true)`
- Double negation: `not(not(bvec2))` should equal original
- Note: `!` operator works on scalar `bool` only, NOT on `bvec2` (per GLSL spec)

### 3. Built-in Functions

**fn-any.glsl**: Test `any()` function

- `any(bvec2)` → `bool` (true if any component is true)
- `any(bvec2(true, false))` → `true`
- `any(bvec2(false, false))` → `false`
- `any(bvec2(true, true))` → `true` (all true case)

**fn-all.glsl**: Test `all()` function

- `all(bvec2)` → `bool` (true only if all components are true)
- `all(bvec2(true, true))` → `true`
- `all(bvec2(true, false))` → `false`
- `all(bvec2(false, false))` → `false` (all false case)

**fn-mix.glsl**: Test `mix()` function with `bvec2`

- `mix(bvec2, bvec2, bvec2)` → `bvec2` (component-wise selection)
- For each component: if selector is `false`, take from first arg; if `true`, take from second arg
- `mix(bvec2(true, false), bvec2(false, true), bvec2(false, true))` → `bvec2(true, true)`

### 4. Constructors

**from-scalar.glsl**: Test scalar broadcast constructor

- `bvec2(bool)` - broadcast single bool to all components
- `bvec2(true)` → `bvec2(true, true)`
- `bvec2(false)` → `bvec2(false, false)`

**from-scalars.glsl**: Test constructors from multiple scalar bools

- `bvec2(bool, bool)` - from 2 bools
- Various combinations of `true`/`false`

**from-shortening.glsl**: Test shortening constructors

- `bvec2(bvec3)` - extract first two components
- `bvec2(bvec4)` - extract first two components
- Verify components are preserved in order

**from-bvec.glsl**: Test identity constructor

- `bvec2(bvec2)` - identity constructor
- Should preserve all components

**from-mixed.glsl**: Test constructors with type conversions

- `bvec2(int, float)` - converts to bool (0/0.0 → false, non-zero → true)
- `bvec2(0, 1.0)` → `bvec2(false, true)`
- Test various numeric inputs

### 5. Conversions

**to-bool.glsl**: Test conversion to scalar bool

- `bool(bvec2)` - extracts first component
- `bool(bvec2(true, false))` → `true` (first component)

**to-int.glsl**: Test conversion to scalar int

- `int(bvec2)` - converts first component (false → 0, true → 1)

**to-uint.glsl**: Test conversion to scalar uint

- `uint(bvec2)` - converts first component (false → 0u, true → 1u)

**to-float.glsl**: Test conversion to scalar float

- `float(bvec2)` - converts first component (false → 0.0, true → 1.0)

**to-ivec.glsl**: Test conversion to ivec2

- `ivec2(bvec2)` - component-wise conversion
- `ivec2(bvec2(true, false))` → `ivec2(1, 0)`

**to-uvec.glsl**: Test conversion to uvec2

- `uvec2(bvec2)` - component-wise conversion
- `uvec2(bvec2(true, false))` → `uvec2(1u, 0u)`

**to-vec.glsl**: Test conversion to vec2

- `vec2(bvec2)` - component-wise conversion
- `vec2(bvec2(true, false))` → `vec2(1.0, 0.0)`

### 6. Assignment

**assign-simple.glsl**: Test simple assignment

- `bvec2 a = bvec2(...); bvec2 b = a;` - assignment
- Verify independence (modifying one doesn't affect the other)
- Self-assignment: `bvec = bvec`

**assign-element.glsl**: Test single component assignment

- `bvec2.x = bool` - assign to single component by name
- `bvec2[0] = bool` - assign to single component by index
- Verify other components unchanged
- Test all components: x, y and indices 0, 1

**assign-swizzle.glsl**: Test multi-component swizzle assignment

- `bvec2.xy = bvec2(...)` - assign to swizzle
- Verify components are updated correctly
- Test various swizzle patterns (no duplicates allowed in assignment)

### 7. Component Access

**access-array.glsl**: Test array-style indexing

- `bvec2[0]`, `bvec2[1]` - array indexing
- Variable indexing: `bvec2[i] `where `i` is computed
- Verify correct component access

**access-component.glsl**: Test component name access

- `bvec2.x`, `bvec2.y` - component access
- Verify correct component values

**access-swizzle.glsl**: Test component swizzling

- `bvec2.xy` → `bvec2` (identity)
- `bvec2.yx` → `bvec2` (reverse)
- `bvec2.xx` → `bvec2` (duplicate)
- `bvec2.yy` → `bvec2` (duplicate)
- Test all name sets: `xy`, `rg`, `st`
- Test various patterns: `xy`, `yx`, `xx`, `yy`, etc.

### 8. Control Flow

**ctrl-if.glsl**: Test `if` statements with bvec2

- `if (any(bvec2))` - condition using any()
- `if (all(bvec2))` - condition using all()
- Note: Control flow conditions must be scalar `bool`, so use `any()` or `all()` to convert

**ctrl-while.glsl**: Test `while` loops with bvec2

- `while (any(bvec2))` - loop condition
- `while (all(bvec2))` - loop condition

**ctrl-for.glsl**: Test `for` loops with bvec2

- `for (init; any(bvec2); update)` - for loop condition

**ctrl-do-while.glsl**: Test `do-while` loops with bvec2

- `do { } while (any(bvec2))` - do-while condition

**ctrl-ternary.glsl**: Test ternary operator with bvec2

- `any(bvec2) ? expr1 : expr2` - ternary with bvec2 condition
- `all(bvec2) ? expr1 : expr2` - ternary with bvec2 condition

### 9. Edge Cases

**edge-nested.glsl**: Test nested bvec2 operations

- `not(equal(bvec2(...), bvec2(...)))`
- `any(not(bvec2(...)))`
- `all(equal(bvec2(...), bvec2(...)))`
- Complex nested expressions

**edge-mixed-components.glsl**: Test various mixed true/false patterns

- Different patterns: `(true, false)`, `(false, true)`, etc.
- Verify component-wise operations work correctly

**edge-all-true.glsl**: Test edge case with all components true

- `bvec2(true, true)` patterns
- Test operations on all-true vectors
- Verify `all()` returns true, `any()` returns true

**edge-all-false.glsl**: Test edge case with all components false

- `bvec2(false, false)` patterns
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

- Logical operators (`&&`, `||`, `^^`, `!`) work on scalar `bool` only, NOT on `bvec2`
- Use `not(bvec2)` built-in instead of `!bvec2`
- Use `any(bvec2)` or `all(bvec2)` to convert `bvec2` to `bool` for control flow
- `==` and `!=` operators return `bool` (aggregate comparison)
- Use `equal()` and `notEqual()` built-ins for component-wise comparison returning `bvec2`

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

Create 32 test files in the flat `bvec2/` directory structure above, with each file containing 3-10 test functions following the vec4/bool pattern. All files use the prefix naming convention:

- `op-*` for operators (comparison)
- `fn-*` for built-in functions (fn-any, fn-all, fn-mix)
- `from-*` for constructors (from-scalar, from-scalars, from-shortening, from-bvec, from-mixed)
- `to-*` for conversions
- `assign-*` for assignments (assign-simple, assign-element, assign-swizzle)
- `access-*` for component access (array, component, swizzle)
- `ctrl-*` for control flow
- `edge-*` for edge cases

## GLSL Spec References

- **operators.adoc**: Constructors (lines 171-229), Vector components (lines 427-579), Equality operators (lines 885-907)
- **builtinfunctions.adoc**: Relational functions (lines 1228-1312), `any()`, `all()`, `not()`, `equal()`, `notEqual()`, `mix()` with `bvec`

