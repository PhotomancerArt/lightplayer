# M4 Matrix Core — Notes

## Goal

Repair shared matrix layout, multiplication, constructors, assignments,
and matrix builtins across q32 targets.

## Current Findings

- Matrix lowering is centralized in `lps-frontend/src/lower_matrix.rs`
  with column-major element indexing `col * rows + row`.
- Binary matrix operations dispatch from `lps-frontend/src/lower_expr.rs`.
- Matrix builtins dispatch through `lps-frontend/src/lower_math_geom.rs`
  (`transpose`, `determinant`, `inverse`, `outerProduct`).
- `lps-frontend/src/lower_access.rs` documents the same column-major
  access shape for `m[col][row]`.
- The report's shared matmul/layout hypothesis is plausible: a single
  flat-order or convention mismatch can affect `op-multiply`,
  assignment variants, constructors, and builtins.
- Transpose, determinant, inverse, and outer-product lowering already
  exist in the current tree, so some report rows may be stale or gated
  by `@unimplemented` markers rather than missing code.
- `builtins/matrix-determinant.glsl` already appears to expect `1.0`
  for the negative diagonal mat4 case, so the old sign expectation is
  likely handled; remaining work is validation/ungating/fixing backend
  failures.
- `outerProduct` needs an explicit convention check for non-square
  matrix dimensions and flat emission order.

## Questions For User

- Confirm the intended Naga/GLSL convention for non-square matrices:
  how should `mat2x3`, `mat3x2`, and `outerProduct(vec2, vec3)` map to
  `TypeInner::Matrix { columns, rows }` and flat storage? **Answered:**
  GLSL column-major semantics are the source of truth.
- If filetest expectations and `lps-q32` reference matrix helpers
  disagree, which should be treated as the starting reference?
- When clearing matrix `@unimplemented` / `@broken` markers, should
  agents remove them per-backend as each target passes, or only after
  all q32 targets pass?

## Implementation Notes

- Fix matrix layout/convention once and reuse it consistently.
- Do not adjust expectations unless independently verified against GLSL
  semantics.
- Hand-verify one small `mat2` multiply, transpose, and `outerProduct`
  against the lowering order before larger edits.
- Fix matmul/layout first; then re-run compound assignment and inverse
  before treating them as independent bugs.

## Validation

- Targeted matrix operation and builtin filetests.
- Suggested targeted groups:
  `matrix/*`, `builtins/matrix-outerproduct.glsl`,
  `builtins/matrix-transpose.glsl`,
  `builtins/matrix-inverse.glsl`, and
  `builtins/matrix-determinant.glsl`.
- Final `just test-filetests`.
