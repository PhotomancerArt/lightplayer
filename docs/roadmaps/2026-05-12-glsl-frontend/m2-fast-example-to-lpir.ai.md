# Milestone 2 - First Example to LPIR

## Title and Goal

Compile `examples/fast/shader.glsl` through typed HIR to LPIR and run it through `rv32lpn.q32`.

## Suggested Plan Location

`docs/roadmaps/2026-05-12-glsl-frontend/m2-fast-example-to-lpir/`

## Scope

In scope:

- Parse function bodies for the minimal straight-line subset.
- Build typed HIR for literals, names, constructors, calls, returns, locals, and binary arithmetic.
- Resolve simple uniforms and function signatures.
- Lower typed HIR to LPIR.
- Register example-used simple builtins such as `mod`.
- Validate LPIR with `lpir::validate_module`.
- Run `examples/fast/shader.glsl` through `rv32lpn.q32` in the filetest seam or an equivalent
  example harness.

Out of scope:

- Branches and loops.
- LPFN noise calls.
- Swizzle and component assignment.
- Full builtin coverage.

## Key Decisions

- HIR is typed and span-carrying from the first lowering milestone.
- The synchronous API must still drive the resumable job internally.
- The first end-to-end proof should target `rv32lpn.q32`, because it combines the new frontend
  with the on-device `lpvm-native` backend path.

## Deliverables

- Minimal parser and HIR nodes for straight-line example code.
- LPIR lowering for the minimal subset.
- A passing end-to-end validation for `examples/fast/shader.glsl`.
- Initial frontend timing output on host.

## Dependencies

- Milestone 1 scaffold and filetest seam.

## Execution Strategy

Full plan. This is the first semantic compiler slice and should capture type representation,
HIR layout, name resolution, and LPIR lowering choices before implementation.
