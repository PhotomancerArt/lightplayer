# Phase 5: Fix Variable Scoping Implementation

## Description

Fix variable shadowing in for loops and if blocks so that inner scope variables correctly shadow
outer scope variables, and outer variables remain unchanged when shadowed.

## Changes

### `lp-glsl/lp-glsl-compiler/src/frontend/semantic/scope.rs`

- **`lookup_variable()` function**: Ensure proper shadowing behavior
    - Inner scope variables should shadow outer scope variables
    - When looking up a variable, find the innermost declaration
    - Verify scope stack is searched correctly (innermost first)

### `lp-glsl/lp-glsl-compiler/src/frontend/codegen/stmt/loop_for.rs`

- **For loop variable scoping**:
    - Ensure init-expression variables are scoped correctly
    - Push new scope for loop body
    - Ensure loop variable shadows outer variables with same name
    - Pop scope after loop body
    - Verify init-expression variable is only in scope until end of loop body

### `lp-glsl/lp-glsl-compiler/src/frontend/codegen/stmt/if_stmt.rs`

- **If block variable scoping**:
    - Ensure inner block variables shadow outer variables
    - Push new scope for if block body
    - Pop scope after if block body
    - Verify variables declared in if block don't leak to outer scope

### `lp-glsl/lp-glsl-compiler/src/frontend/codegen/context.rs`

- **Scope management**: Verify scope stack is managed correctly
    - Check `push_scope()` and `pop_scope()` usage
    - Ensure scopes are pushed/popped at correct points
    - Verify scope stack state is correct throughout codegen

## Success Criteria

- Variable shadowing works correctly in for loops
- Variable shadowing works correctly in if blocks
- Outer variables remain unchanged when shadowed
- Test `test_for_loop_init_shadowing()` passes (expects 100)
- Test `test_if_variable_shadowing()` passes (expects 5)
- All variable scoping tests pass
- No regressions in other scoping tests

## Implementation Notes

- GLSL spec: inner scope variables shadow outer scope variables
- Scope stack should be searched from innermost to outermost
- For loops: init-expression variables are scoped to loop body
- If blocks: variables declared in block are scoped to block
- Verify scope management is consistent across all control flow constructs
