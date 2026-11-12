# Milestone 4 - LPFN Calls and Rocaille Control Flow

## Title and Goal

Finish the example compatibility surface by supporting LPFN calls, out/inout arguments, nested
loops, compound assignment, and component assignment.

## Suggested Plan Location

`docs/roadmaps/2026-05-12-glsl-frontend/m4-lpfn-and-rocaille/`

## Scope

In scope:

- LPFN builtin registry entries for `lpfn_fbm`, `lpfn_hsv2rgb`, `lpfn_psrdnoise`, and
  `lpfn_worley`.
- Out/inout argument copy-in/copy-out semantics needed by `lpfn_psrdnoise`.
- `for` loops with local loop variables, comparisons, increments, and nested loop bodies.
- Compound assignment such as `+=`.
- Component assignment such as `color.a = 1.0`.
- Additional builtins needed by `rocaille`: `length` and `tanh`.
- End-to-end validation for every current example shader through `rv32lpn.q32`, with side-by-side
  comparison against `rv32n.q32` where useful.

Out of scope:

- General arrays, structs, textures, and matrices.
- Arbitrary GLSL overload edge cases not used by examples.
- Production default switch-over.

## Key Decisions

- LPFN functions are resolved through compact typed builtin/import tables, not source-injected
  declarations.
- Loop support should lower from structured HIR to LPIR control flow, preserving spans for errors.
- Out/inout semantics belong in HIR/sema, not as ad hoc LPIR call fixups.

## Deliverables

- All current example shaders compile and run through emulator validation.
- LPFN registry and tests for every example-used signature.
- Host differential output checks against the Naga path for selected sample calls or rendered
  points.

## Dependencies

- Milestone 3 example language core.

## Execution Strategy

Full plan. LPFN and loop semantics are central to correctness and deserve phase-level design before
implementation.
