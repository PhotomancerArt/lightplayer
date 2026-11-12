# Milestone IV: Array type lowering

## Goal

Array declarations, indexing, assignment, and function parameters work in LPIR on `jit.q32`.
Files under `array/` and `const/array-size/` that currently fail with `unsupported type for LPIR:
Array { … }` pass or are annotated for known limitations.

## Suggested plan name

`lpir-parity-milestone-iv`

## Scope

**In scope:**

- **Type mapping**: extend `naga_type_to_ir_types` in `lower_ctx.rs` to flatten fixed-size arrays
  to N × element IR types (e.g. `float[5]` → 5 `f32` VRegs, `vec3[4]` → 12 `f32` VRegs).
- **Stack slot layout**: arrays should lower to LPIR stack slots with element-stride addressing.
  `SlotAddr` + constant offset for static indexing; `SlotAddr` + computed offset for dynamic
  indexing.
- **Load / Store**: array element access through `AccessIndex` (constant) and `Access` (dynamic)
  on array-typed locals.
- **Const array sizes**: `const/array-size/const-int.glsl`, `local.glsl`, `param.glsl` — the Naga
  constant evaluator provides the size; the LPIR lowering must accept it.
- **Function parameters**: `function/param-out-array.glsl` — arrays as `out` params through stack
  slots + copy-back (same mechanism as vector `out` params but wider).
- **`array/phase/1-foundation.glsl`**, `2-bounds-checking.glsl`, `4-vector-matrix-elements.glsl`
  — progressive array feature tests.

**Out of scope:**

- Multidimensional arrays, array-of-struct, array equality operators (still `@unimplemented` if
  they exist in the corpus).
- Struct types (separate roadmap).
- Dynamic array sizes (GLSL 4.50 arrays are fixed-size; Naga enforces this).

## Key decisions

- **Scalarized vs slot-based:** Small arrays of scalars/vectors can be scalarized to VRegs (like
  matrices). Larger arrays or arrays used with dynamic indexing need stack slots. Decision: use
  stack slots uniformly for arrays — simpler, consistent, and dynamic indexing requires it anyway.
  The optimizer can promote to VRegs later if beneficial.
- **Bounds checking:** GLSL spec says out-of-bounds is undefined behavior. For Q32/embedded, we
  can either trap or clamp. Decision: follow existing LPIR behavior for slot access (no bounds
  check in v1; document as UB).

## Deliverables

- Updated `lower_ctx.rs` (type mapping for arrays).
- Updated `lower_expr.rs` / `lower_stmt.rs` (array access, assignment).
- New or updated slot-addressing helpers in lowering.
- ~5+ filetest files passing; many `@unimplemented` array files may also start passing.

## Dependencies

Milestones I–II (relational, pointer stores) — some array test files also use bvec or matrix
operations. Array lowering itself is independent of matrix invoke (Milestone V).

## Estimated scope

Medium–large. This is the most exploratory milestone — array layout in LPIR slots, element
addressing, and `out`-param copy-back are new ground. Expect ~150-300 lines of lowering logic
plus test iteration.
