# Design: Fix Filetests Low Hanging Fruit & Test Framework Bugs

## Overview

Fix high-priority bugs identified in filetests failure analysis: variable scoping, precision
tolerances, vec comparison functions, and vec conversion.

## File Structure

```
lp-glsl/lp-glsl-filetests/filetests/
├── control/
│   ├── for/variable-scope.glsl              # UPDATE: Fix test expectations
│   └── if/variable-scope.glsl               # UPDATE: Fix test expectations
├── builtins/
│   ├── angle-degrees.glsl                   # UPDATE: Add tolerances
│   ├── angle-radians.glsl                   # UPDATE: Add tolerances
│   ├── common-roundeven.glsl                 # UPDATE: Add tolerance
│   ├── edge-component-wise.glsl              # UPDATE: Add tolerances
│   ├── edge-exp-domain.glsl                 # UPDATE: Add tolerances
│   ├── edge-nan-inf-propagation.glsl        # UPDATE: Add tolerances
│   ├── edge-precision.glsl                  # UPDATE: Add tolerances
│   ├── edge-trig-domain.glsl                # UPDATE: Add tolerances
│   └── matrix-determinant.glsl               # UPDATE: Add tolerance
└── vec/
    ├── vec2/fn-equal.gen.glsl                # UPDATE: Fix test (investigate equal() bug)
    └── uvec2/from-scalars.glsl               # UPDATE: Fix test expectation (uint conversion)

lp-glsl/lp-glsl-compiler/src/
├── frontend/
│   ├── codegen/
│   │   ├── builtins/
│   │   │   └── relational.rs                 # UPDATE: Fix equal() for bvec2 arguments
│   │   └── expr/
│   │       └── coercion.rs                   # UPDATE: Fix uint() cast to wrap negatives
│   └── semantic/
│       └── scope.rs                          # UPDATE: Fix variable shadowing in for/if
│   └── codegen/
│       ├── stmt/
│       │   ├── loop_for.rs                    # UPDATE: Fix variable scoping in for loops
│       │   └── if_stmt.rs                     # UPDATE: Fix variable scoping in if blocks
│       └── context.rs                        # UPDATE: Ensure proper scope handling
└── backend/
    └── transform/
        └── q32/
            └── converters/
                └── conversions.rs             # UPDATE: Fix fcvt_to_uint to wrap negatives
```

## Types and Functions

### Test File Updates

```
variable-scope.glsl (for/if) - # UPDATE: Fix test expectations
├── test_for_loop_init_shadowing() - # UPDATE: Expect 100 instead of 3
└── test_if_variable_shadowing() - # UPDATE: Expect 5 instead of 10

angle-degrees.glsl, angle-radians.glsl, etc. - # UPDATE: Add tolerances
└── Test directives - # UPDATE: Add (tolerance: 0.001) or similar to ~= comparisons

fn-equal.gen.glsl - # UPDATE: May need test fix after equal() bug is fixed
└── test_vec2_equal_function_in_expression() - # UPDATE: Verify after fix

from-scalars.glsl (uvec2) - # UPDATE: Test expectation already correct, verify after fix
└── test_uvec2_from_scalars_function_results() - # UPDATE: Verify uint(-3.2) wraps correctly
```

### Compiler Code Updates

```
Scope - # UPDATE: Variable shadowing implementation
├── push_scope() - # EXISTING: Push new scope
├── pop_scope() - # EXISTING: Pop scope
└── lookup_variable() - # UPDATE: Ensure proper shadowing (inner shadows outer)

builtin_equal() - # UPDATE: Fix bvec2 argument handling
├── Handle bvec2 arguments correctly - # UPDATE: Ensure comparison works for boolean vectors
└── Return type handling - # UPDATE: Verify return type is correct for bvec2

coercion.rs - # UPDATE: Fix uint() cast
└── float_to_uint() - # UPDATE: Wrap negative values instead of clamping to 0

fcvt_to_uint() - # UPDATE: Fix q32 backend conversion
└── Negative value handling - # UPDATE: Wrap instead of clamp

loop_for.rs - # UPDATE: Fix variable scoping
├── Handle init-expression variable scope - # UPDATE: Ensure loop variable shadows outer
└── Scope management - # UPDATE: Properly push/pop scopes for loop variables

if_stmt.rs - # UPDATE: Fix variable scoping
└── Scope management - # UPDATE: Ensure inner scope variables shadow outer
```

## Implementation Details

### Phase 1: Fix Test Expectations

1. **Fix `control/for/variable-scope.glsl`**:
    - Line 31: Change `test_for_loop_init_shadowing() == 3` to `== 100`
    - Fix comment if needed to match expectation

2. **Fix `control/if/variable-scope.glsl`**:
    - Line 38: Change `test_if_variable_shadowing() == 10` to `== 5`
    - Fix comment if needed to match expectation

3. **Remove invalid test**:
    - `control/for/variable-scope.glsl` line 92-101: Remove or fix invalid GLSL syntax test (
      `int j = i < 3` in for condition)

### Phase 2: Add Precision Tolerances

Add explicit tolerances to precision-sensitive tests:

- `builtins/angle-degrees.glsl`: Add `(tolerance: 0.001)` to all `~=` comparisons
- `builtins/angle-radians.glsl`: Add `(tolerance: 0.001)` to all `~=` comparisons
- `builtins/common-roundeven.glsl`: Add tolerance to failing test
- `builtins/edge-*` tests: Add appropriate tolerances
- `builtins/matrix-determinant.glsl`: Add tolerance to failing test

### Phase 3: Fix equal() Function for bvec2

1. **Investigate `builtin_equal()` in `relational.rs`**:
    - Check how bvec2 arguments are handled
    - Verify comparison logic for boolean vectors
    - Ensure return type is correct

2. **Fix implementation**:
    - Ensure bvec2 arguments are compared correctly
    - Verify nested calls work (e.g., `equal(equal(a, b), equal(b, c))`)

### Phase 4: Fix uint() Cast for Negative Values

1. **Fix `coercion.rs`**:
    - Update `float_to_uint()` conversion to wrap negative values
    - Use modulo 2^32 wrapping instead of clamping to 0

2. **Fix `conversions.rs` (q32 backend)**:
    - Update `fcvt_to_uint()` to wrap negative values
    - Remove clamping logic, add wrapping logic

### Phase 5: Fix Variable Scoping

1. **Fix `scope.rs`**:
    - Ensure `lookup_variable()` respects shadowing (inner shadows outer)
    - Verify scope stack is managed correctly

2. **Fix `loop_for.rs`**:
    - Ensure init-expression variables are scoped correctly
    - Ensure loop variable shadows outer variables
    - Properly push/pop scopes

3. **Fix `if_stmt.rs`**:
    - Ensure inner block variables shadow outer variables
    - Properly push/pop scopes for if blocks

## Success Criteria

1. All test expectation fixes applied correctly
2. Precision tolerances added to all affected tests
3. `equal()` function works correctly with bvec2 arguments
4. `uint()` cast wraps negative values correctly
5. Variable shadowing works correctly in for loops and if blocks
6. All affected tests pass
7. Code compiles without errors
8. No warnings introduced
