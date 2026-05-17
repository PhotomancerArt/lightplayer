# M1: Control Flow, Operators, and Lvalues

## Objective

Turn the current example-oriented parser into a solid scalar/vector language core. This milestone should make common shader control flow and assignment forms boring.

## Features

- block scopes and variable shadowing
- `while`, `do while`, and `for`
- `break` and `continue`
- empty statements
- logical operators with correct short-circuit behavior
- ternary expressions if present in filetests/examples
- compound assignment
- prefix/postfix increment and decrement
- comparison/equality operators across scalar and vector cases covered by filetests
- assignment to swizzles where legal
- assignment through a general lvalue representation, initially for locals and vector components

## Implementation Notes

Add a small lvalue abstraction early:

```text
base + path components
```

Path components should cover swizzle and later member/index access. M1 only needs to lower the pieces required for scalar/vector tests, but the type shape should not assume swizzle is the only projection.

Loop lowering should be straightforward LPIR control-flow emission. Prefer a small loop context stack over special-casing `break` and `continue` at parse time.

Short-circuiting should lower as control flow, not eager arithmetic, so later WGSL-like semantics are not boxed in.

## Filetest Gate

Start narrow:

```bash
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise lps-glsl scalar vec operators control
```

Then add targeted fixtures for:

- nested loops
- nested `if` inside loops
- component assignment
- compound assignment
- short-circuit expressions with side effects

## Done

- existing `lps-glsl` fixtures still pass
- the broad scalar/vector/operator/control slice passes or has documented intentional skips
- diagnostics for parse/semantic failures show source line and span
- no Naga dependency is introduced into the default firmware path
