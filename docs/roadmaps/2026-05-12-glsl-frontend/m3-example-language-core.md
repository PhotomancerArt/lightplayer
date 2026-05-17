# Milestone 3 - Example Language Core

## Title and Goal

Support the scalar/vector, conditional, swizzle, and call features needed by `basic`, `basic2`,
`noise.fx`, and the perf examples.

## Suggested Plan Location

`docs/roadmaps/2026-05-12-glsl-frontend/m3-example-language-core/`

## Scope

In scope:

- Function calls before definitions and reachable-function ordering.
- Global and local `const` declarations.
- `if`, `else`, and early-return chains.
- Scalar/vector constructors and casts, including `float(i)` and `int(a)`.
- Component reads and swizzles: `.x`, `.y`, `.xy`, `.yx`, etc.
- Local assignment and simple lvalue handling.
- Builtins used by examples outside LPFN noise: `abs`, `atan`, `clamp`, `cos`, `exp`, `floor`,
  `fract`, `max`, `min`, `mix`, `mod`, `sin`, and `smoothstep`.
- Differential validation against selected filetests for each supported feature.

Out of scope:

- Nested `for` loops from `rocaille`.
- Component assignment and compound assignment.
- LPFN out/inout semantics.
- Textures, structs, arrays, and matrices.

## Key Decisions

- Filetests are sampled by feature, not treated as the roadmap target.
- Reachability-first compilation is used for example builds.
- Strict host validation can still compile more source when useful for diagnostics.

## Deliverables

- `examples/basic2/shader.glsl` compiles and runs through `rv32lpn.q32`.
- `examples/basic/shader.glsl`, `examples/noise.fx/main.glsl`, and perf examples compile through
  the non-loop, non-out/inout portions needed before LPFN finalization.
- Feature-specific filetest subset documented in the milestone summary.

## Dependencies

- Milestone 2 typed HIR and LPIR lowering.

## Execution Strategy

Full plan. This milestone adds most of the semantic surface area and needs careful decomposition
across parser, sema, HIR, lowering, and tests.
