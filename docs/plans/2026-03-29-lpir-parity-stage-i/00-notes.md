# Plan notes — LPIR parity stage I (relational expressions)

Roadmap: `docs/roadmaps/2026-03-29-lpir-parity/milestone-i-relational-expressions.md`

**Normative Q32 semantics:** [`docs/design/q32.md`](../../design/q32.md) — single source of truth for
fixed-point behavior (including §6 relational builtins: `isnan` / `isinf` always false, `all` /
`any` / `not` as specified).

## Scope of work

- Fix type inference for `Expression::Relational` in `expr_scalar.rs` (`expr_type_inner`, and
  `expr_scalar_kind` where wrong).
- Ensure `lower_expr.rs` lowering matches GLSL semantics for `all` / `any` / `isnan` / `isinf` on
  vectors (and matrix equality that lowers through relational + compare).
- Rewrite `builtins/common-isnan.glsl` and `builtins/common-isinf.glsl` to avoid literals Naga
  rejects (e.g. infinite float).
- Validate with targeted filetests: **Tier A** list in
  [`expected-passing-tests.md`](./expected-passing-tests.md) on **`jit.q32`**, **`wasm.q32`**, and
  **`rv32.q32`**; Tier B (`vec/bvec*/`, etc.) enumerated in `summary.md` when triaged. Plus
  `cargo check` for `fw-esp32` / `fw-emu` per workspace rules.

## Current state of the codebase

### `expr_type_inner` (`expr_scalar.rs`)

There is **no** `Expression::Relational` arm. The match ends with `Expression::Math` then `_ =>
Err("expr_type_inner unsupported …")`. Any code path that needs the **type** of a relational
expression (shape for `all`/`any` result, `isnan`/`isinf` result vector, or nested use inside
`Select`, stores, etc.) will fail before lowering.

### `expr_scalar_kind` (`expr_scalar.rs`)

For `RelationalFunction::All | Any | IsNan | IsInf`, the implementation delegates to
`expr_scalar_kind(module, func, *argument)`.

- **`all` / `any`**: GLSL result type is **bool scalar**, while the argument is **bvecN**. Returning
  the argument’s kind is wrong (`Bool` for a vector is still the element kind, but callers that
  expect “result is scalar bool” may be confused; the real bug is likely **`expr_type_inner`**
  missing entirely).
- **`isnan` / `isinf`**: Result type is **bvecN** for vec argument (same size as float vec). Using
  the float argument’s scalar kind is incorrect for “result is bool vector” shape inference when
  `expr_type_inner` is fixed to return a bool vector.

### `lower_relational` (`lower_expr.rs`)

- **`All` / `Any`**: Lowers bool vector lanes with `Iand` / `Ior` chain; returns **one** `VReg`
  (scalar bool). Aligns with GLSL.
- **`IsNan` / `IsInf`**: Per-lane lowering today uses `Fne` self and div-by-zero sentinel compares.
  **Normative Q32** ([`q32.md` §6](../../design/q32.md)): both builtins are **always false**; div0
  saturation raw values must **not** be treated as infinity by `isinf`. Lowering should be updated
  to match.

### Test files

`common-isnan.glsl` / `common-isinf.glsl` may still use `1.0/0.0` or similar; Naga rejects infinite
literals—rewrites are required per milestone.

## Questions (to iterate with the user)

### Q1 — `isnan` / `isinf` on Q32 fixed-point

**Resolved** by normative [`docs/design/q32.md`](../../design/q32.md) §6: **`isnan` and `isinf` are
always false** on Q32; saturation artifacts are not `isinf`. Implementation in `lower_expr.rs` should
conform (remove sentinel-based `isinf` and `Fne`-self `isnan` for the Q32 path).

### Q2 — Matrix `==` / `!=` coverage

**Resolution (adopted):** Naga lowers matrix `==` / `!=` as component-wise compare → bool vector →
`Relational::All` (or equivalent). `lower_binary_vec` already emits per-lane `Feq` / `Fne`. The
main fix is **type inference** on `Relational`, not a new matrix compare path. Validate with
`matrix/mat{2,3,4}/op-equal.glsl` and `op-not-equal.glsl`; treat unrelated failures as out of scope.

### Q3 — Baseline `@unimplemented` on relational files

**Resolution (adopted):** Remove `@unimplemented(backend=jit)` from files this milestone targets
once fixes land; confirm they pass unmarked.

## # Notes

- **2026-03-29:** Q32 behavior questions defer to [`docs/design/q32.md`](../../design/q32.md)
  (normative). Stage I relational lowering must match §6.
- **2026-03-29:** Q2/Q3 closed; `00-design.md` and phase files added.
