# Milestone I: Relational expressions

## Goal

All filetest files that fail due to `Relational` expression handling pass on `jit.q32`.

## Suggested plan name

`lpir-parity-milestone-i`

## Scope

**In scope:**

- Fix `expr_type_inner` (or equivalent) in `lp-glsl-naga/src/expr_scalar.rs` so it returns a
  valid type for `Expression::Relational { All, Any, Not, IsNan, IsInf }`. The phase-8 fix was
  partial — some callers still hit the unsupported path.
- Ensure `lower_expr.rs` correctly decomposes `Relational` to scalarized ops:
  - `All` on bvecN → `iand` chain on components
  - `Any` on bvecN → `ior` chain on components
  - `Not` on bvecN → per-component `ieq` with 0
  - `IsNan` / `IsInf` — component-wise; Q32 per [`docs/design/q32.md`](../../design/q32.md) §6
    (always false; div0 saturation values are not Inf)
- Matrix `==` / `!=` desugars through `Relational::All` over component-wise comparison — unblocked
  once `All` works.
- Rewrite `builtins/common-isnan.glsl` and `common-isinf.glsl` to avoid `1.0/0.0` literal that
  Naga rejects as `Float literal is infinite`.

**Out of scope:**

- Bvec dynamic indexing (`Load from non-local pointer`) — Milestone II.
- Bvec casts / `mix(bvec)` — Milestone III.
- Matrix element stores — Milestone II.
- Array / struct types — Milestones IV / deferred.

## Key decisions

- **Normative reference:** [`docs/design/q32.md`](../../design/q32.md) (§6 relational builtins, §7
  `@unsupported` policy). `IsNan` / `IsInf` on Q32: always `false`; do not expose div-by-zero
  saturation encodings as infinity.

## Deliverables

- Updated `expr_scalar.rs`, `lower_expr.rs` in `lp-glsl-naga`.
- Rewritten `common-isnan.glsl`, `common-isinf.glsl` (avoid unparseable literals).
- **Explicit test corpus + three-target bar:** see
  [`docs/plans/2026-03-29-lpir-parity-stage-i/expected-passing-tests.md`](../../plans/2026-03-29-lpir-parity-stage-i/expected-passing-tests.md)
  (Tier A = 8 files; Tier B = relational-only `vec/bvec*` / related, listed in plan `summary.md`).
  Tier A must pass on **`jit.q32`**, **`wasm.q32`**, and **`rv32.q32`** with no blocking
  `@unimplemented(backend=…)` unless truly out of scope.

## Dependencies

Optional: run `--mark-unimplemented` (single target, e.g. `jit.q32`) so the suite is green before
you touch relational lowering; then remove only the annotations for tests you expect this milestone
to fix. Otherwise you will see the full set of unrelated failures alongside relational work.

## Estimated scope

Small–medium. ~50-100 lines of lowering logic; most complexity is in understanding the Naga
expression tree, not in the generated IR.
