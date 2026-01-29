# Plan: Document Existing Control Flow Tests

## Overview

This plan documents the existing control flow tests in `lightplayer/crates/lp-glsl-filetests/filetests/control/`. These tests cover if/else statements, loops (for/while/do-while), break/continue statements, return statements, and nested control flow.

## Existing Test Structure

The control flow tests are organized in subdirectories:

```
control/
├── if/                    (if statements)
│   ├── basic.glsl
│   ├── compound.glsl
│   ├── nested.glsl
│   └── variable-scope.glsl
├── if_else/               (if-else statements)
│   ├── basic.glsl
│   ├── chained.glsl
│   ├── compound.glsl
│   └── nested.glsl
├── loop_for/              (for loops)
│   ├── basic.glsl
│   ├── complex-condition.glsl
│   ├── decrement.glsl
│   ├── empty-body.glsl
│   ├── nested.glsl
│   └── variable-scope.glsl
├── loop_while/            (while loops)
│   ├── basic.glsl
│   ├── complex-condition.glsl
│   ├── empty-body.glsl
│   ├── nested.glsl
│   └── variable-scope.glsl
├── loop_do_while/         (do-while loops)
│   ├── basic.glsl
│   ├── nested.glsl
│   ├── runs-at-least-once.glsl
│   └── variable-scope.glsl
├── loop_break/            (break statements)
│   ├── do-while-loop.glsl
│   ├── for-loop.glsl
│   ├── nested.glsl
│   └── while-loop.glsl
├── loop_continue/         (continue statements)
│   ├── do-while-loop.glsl
│   ├── for-loop.glsl
│   ├── nested.glsl
│   └── while-loop.glsl
├── return/                (return statements)
│   ├── conditional.glsl
│   ├── early.glsl
│   ├── edge-cases.glsl
│   ├── void.glsl
│   └── with-value.glsl
├── nested/                (nested control flow)
│   ├── complex.glsl
│   ├── if-in-loop.glsl
│   └── loop-in-if.glsl
└── edge_cases/            (edge cases)
    ├── break-continue-edge-cases.glsl
    ├── compound-statements.glsl
    ├── condition-expressions.glsl
    ├── empty-statements.glsl
    ├── loop-expression-scope.glsl
    ├── non-terminating.glsl
    ├── single-statement.glsl
    └── variable-shadowing.glsl
```

## Test Coverage

### If Statements (`if/`)

**basic.glsl**: Basic if statement functionality

- Simple if with true/false conditions
- Conditional execution

**compound.glsl**: Compound statements in if

- If with block statements
- Multiple statements in if body

**nested.glsl**: Nested if statements

- If within if
- Multiple levels of nesting

**variable-scope.glsl**: Variable scope in if statements

- Variables declared in if scope
- Scope rules for if statements

### If-Else Statements (`if_else/`)

**basic.glsl**: Basic if-else functionality

- If-else with true/false conditions
- Else branch execution

**chained.glsl**: Chained if-else statements

- Multiple if-else-if chains
- Final else clause

**compound.glsl**: Compound statements in if-else

- Block statements in both branches
- Multiple statements per branch

**nested.glsl**: Nested if-else statements

- If-else within if-else
- Complex nesting patterns

### For Loops (`loop_for/`)

**basic.glsl**: Basic for loop functionality

- Standard for loop with init, condition, update
- Loop iteration

**complex-condition.glsl**: Complex loop conditions

- Multiple conditions
- Complex boolean expressions

**decrement.glsl**: Decrementing for loops

- Loops that count down
- Negative increments

**empty-body.glsl**: For loops with empty bodies

- Loops that only perform side effects in header
- No body statements

**nested.glsl**: Nested for loops

- For loop within for loop
- Multiple nesting levels

**variable-scope.glsl**: Variable scope in for loops

- Variables declared in init-expression
- Variables declared in condition-expression
- Scope rules for loop variables

### While Loops (`loop_while/`)

**basic.glsl**: Basic while loop functionality

- Standard while loop
- Loop iteration

**complex-condition.glsl**: Complex while conditions

- Multiple conditions
- Complex boolean expressions

**empty-body.glsl**: While loops with empty bodies

- Loops that only check condition
- No body statements

**nested.glsl**: Nested while loops

- While loop within while loop
- Multiple nesting levels

**variable-scope.glsl**: Variable scope in while loops

- Variables declared in condition-expression
- Scope rules for loop variables

### Do-While Loops (`loop_do_while/`)

**basic.glsl**: Basic do-while loop functionality

- Standard do-while loop
- Body executes before condition check

**nested.glsl**: Nested do-while loops

- Do-while loop within do-while loop
- Multiple nesting levels

**runs-at-least-once.glsl**: Do-while always runs once

- Body executes even if condition is false initially
- Guaranteed execution

**variable-scope.glsl**: Variable scope in do-while loops

- Variables declared in body
- Scope rules (cannot declare in condition-expression)

### Break Statements (`loop_break/`)

**for-loop.glsl**: Break in for loops

- Early exit from for loop
- Break with conditions
- Break in nested loops

**while-loop.glsl**: Break in while loops

- Early exit from while loop
- Break with conditions

**do-while-loop.glsl**: Break in do-while loops

- Early exit from do-while loop
- Break with conditions

**nested.glsl**: Break in nested loops

- Break exits innermost loop
- Multiple levels of nesting

### Continue Statements (`loop_continue/`)

**for-loop.glsl**: Continue in for loops

- Skip to loop-expression, then condition-expression
- Continue with conditions
- Continue in nested loops

**while-loop.glsl**: Continue in while loops

- Skip to condition-expression
- Continue with conditions

**do-while-loop.glsl**: Continue in do-while loops

- Skip to condition-expression
- Continue with conditions

**nested.glsl**: Continue in nested loops

- Continue affects innermost loop
- Multiple levels of nesting

### Return Statements (`return/`)

**conditional.glsl**: Conditional return statements

- Return in if statements
- Return in loops

**early.glsl**: Early return from functions

- Return before end of function
- Multiple return paths

**edge-cases.glsl**: Return statement edge cases

- Return in nested scopes
- Return with various types

**void.glsl**: Void return statements

- Return without value
- Return in void functions

**with-value.glsl**: Return with values

- Return with scalar values
- Return with vector/matrix values

### Nested Control Flow (`nested/`)

**complex.glsl**: Complex nested control flow

- Multiple levels of nesting
- Mixed control structures

**if-in-loop.glsl**: If statements within loops

- If within for/while/do-while
- Conditional execution in loops

**loop-in-if.glsl**: Loops within if statements

- For/while/do-while within if
- Conditional loop execution

### Edge Cases (`edge_cases/`)

**break-continue-edge-cases.glsl**: Edge cases for break/continue

- Break/continue in various contexts
- Edge case behaviors

**compound-statements.glsl**: Compound statement edge cases

- Block scoping
- Statement grouping

**condition-expressions.glsl**: Condition expression edge cases

- Complex conditions
- Type conversions in conditions

**empty-statements.glsl**: Empty statement edge cases

- Semicolon-only statements
- Empty bodies

**loop-expression-scope.glsl**: Loop expression scope edge cases

- Variable scope in loop expressions
- Scope boundaries

**non-terminating.glsl**: Non-terminating loops

- Infinite loops
- Platform-dependent behavior

**single-statement.glsl**: Single statement edge cases

- Loops/if with single statements
- No braces needed

**variable-shadowing.glsl**: Variable shadowing edge cases

- Shadowing in nested scopes
- Name resolution

## Missing Coverage

The following control flow features are NOT yet covered:

1. **Switch statements** - See `control-flow-switch.md`
2. **Discard statement** - See `control-flow-discard.md`

## GLSL Spec References

- **statements.adoc**: Selection (lines 525-604), Iteration (lines 606-678), Jumps (lines 680-744)
- Key sections:
  - If/else statements
  - For/while/do-while loops
  - Break/continue statements
  - Return statements
  - Scope rules for control flow
  - Variable declarations in control flow





