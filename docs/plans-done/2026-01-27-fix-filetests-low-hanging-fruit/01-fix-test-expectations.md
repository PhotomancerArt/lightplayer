# Phase 1: Fix Test Expectations for Variable Scoping

## Description

Fix test expectations in variable scoping tests to match GLSL semantics. In GLSL, inner scope variables shadow outer scope variables, so outer variables should remain unchanged when shadowed.

## Changes

### `lp-glsl/crates/lp-glsl-filetests/filetests/control/for/variable-scope.glsl`

- **Line 31**: Change `test_for_loop_init_shadowing() == 3` to `== 100`
  - The outer `i` should remain 100 (shadowed by loop variable)
  - Update comment on line 27 if needed to match expectation

- **Lines 92-101**: Remove or fix invalid GLSL syntax test
  - Test `test_for_loop_condition_declaration()` tries to declare variable in condition: `for (int i = 0; int j = i < 3; i++)`
  - This is invalid GLSL syntax - either remove test or fix to valid syntax

### `lp-glsl/crates/lp-glsl-filetests/filetests/control/if/variable-scope.glsl`

- **Line 38**: Change `test_if_variable_shadowing() == 10` to `== 5`
  - The outer `x` should remain 5 (shadowed by inner `x`)
  - Update comment on line 33 if needed to match expectation

## Success Criteria

- Test expectations match GLSL semantics (outer variables unchanged when shadowed)
- Invalid GLSL syntax test removed or fixed
- Tests compile correctly
- Comments match expectations

## Implementation Notes

- Verify that the test expectations align with GLSL spec
- Ensure comments accurately describe the expected behavior
- Remove tests with invalid syntax rather than trying to make them work
