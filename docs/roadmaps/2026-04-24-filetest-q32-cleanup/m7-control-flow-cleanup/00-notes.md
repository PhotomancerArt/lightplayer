# M7 Control Flow Cleanup — Notes

## Goal

Retire residual control-flow failures after earlier frontend and
aggregate fixes have landed.

## Current Findings

- Expected residual files are `control/ternary/types.glsl` and
  `control/edge_cases/loop-expression-scope.glsl`.
- Struct ternary currently likely flows through scalar/vector
  `Expression::Select` lowering plus aggregate slot/copy behavior. If
  it still fails after M3, the bug may be aggregate copy/phi layout
  rather than generic control flow.
- The loop-expression-scope failure likely involves Naga
  `Statement::Loop` lowering and GLSL for-loop semantics where the body
  and step see the same loop variable.
- M7 should start with a re-run because earlier aggregate/frontend work
  may retire some or all Section F markers.

## Questions For User

- If `control/ternary/types.glsl` still differs by a small q32 numeric
  amount after M3, should the agent treat that as numeric tolerance /
  expectation work or strict aggregate-copy behavior until proven
  otherwise?

## Implementation Notes

- Re-run Section F before implementing; earlier milestones may retire
  aggregate ternary failures.
- Avoid broad CFG refactors unless the refreshed failures prove the
  current lowering model is wrong.

## Validation

- Targeted control-flow filetests.
- Key files:
  `control/ternary/types.glsl` and
  `control/edge_cases/loop-expression-scope.glsl`.
- Final `just test-filetests`.
