# M0: Prep, Incremental Contracts, and Scaffolding

## Objective

Add the rails that make M1 safer: explicit resumability tests, a first-class lvalue home, and only the organization work needed by the next feature slice.

This milestone should stay small. It should make future edits easier without delaying language coverage for a broad module migration.

## Features

- Compile job contract tests for coarse resumability
- same-output check for single-step execution versus default-budget execution
- explicit behavior for zero-step budgets
- failure behavior for malformed shaders
- initial lvalue module/type scaffold
- small exports/module wiring needed by M1

## Implementation Notes

The current compile job yields across coarse stages:

```text
Lex -> Index -> Body -> Lower -> Done
```

M0 should preserve that behavior and make it observable in tests. Later milestones can improve granularity inside `Body` and `Lower`, but should not accidentally remove the resumable shape.

The lvalue scaffold can be intentionally small:

```text
base + projection path
```

Projection path should be able to grow from swizzles to fields and indexes. M0 does not need to lower all path forms yet.

Avoid a large parser/HIR split unless the first M1 edits make it necessary.

## Filetest Gate

Keep the current lps-glsl gate green:

```bash
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise lps-glsl
```

Run crate tests:

```bash
cargo test -p lps-glsl
```

## Done

- `CompileJob` resumability behavior is covered by focused unit tests
- the lvalue concept has a small module home
- the current lps-glsl filetests still pass
- no behavior changes are made beyond test/scaffold work

