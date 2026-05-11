# M3 — Frontend overloads & l-values: scoped plan (2026-04-24)

Validation targets for this pass: **wasm.q32**, **rv32c.q32**, **rv32n.q32** only (jit deprecated).

## Current scope (this milestone slice)

Per roadmap decisions:

- **No** general `out` / `inout` lowering for `Access` / `AccessIndex` l-values (array elements, struct fields, nested paths). Deferred to a dedicated milestone; see `../deferred-access-lvalue-out-inout.md`.
- **No** Naga fork/patch; **bvecN `mix`** stays **@unsupported** with note if resolver ambiguity remains.
- Safe, localized fixes only: parser workarounds that do not fork Naga, overload/call issues, aggregate/array behavior not covered by deferred access-lvalue.

## Checklist — Section A (`docs/reports/2026-04-23-filetest-triage/broken.md`)

| Item | Status (this tree) | Notes |
|------|--------------------|--------|
| `vec/bvec2\|bvec3\|bvec4/fn-mix.glsl` | **@unsupported** (all q32 targets) | Naga ambiguous `mix` for bvecN; no fork per decision. |
| `function/edge-const-out-error.glsl` | **Pass** (5/5) | `const` / `in` parameter order handled in `lps-frontend` `parse.rs` (normalize to `const T` before Naga). |
| `function/edge-lvalue-out.glsl` | **Deferred** | Compile fails on access-shaped `out`/`inout` cases (e.g. array element); plain locals covered when file compiles — blocked on deferred milestone. |
| `function/edge-return-type-match.glsl` | **Pass** (10/10 × 3 targets) | **Fix:** array local assignment from aggregate **call result** (`Expression::CallResult`): `copy_stack_array_slots` in `lower_stmt.rs`. Removed stale `@broken` annotations. |
| `function/declare-prototype.glsl` | **Pass** (4/4 × 3) | No change this pass; was already green (M2 harness / baseline). |
| `function/overload-same-name.glsl` | **Pass** (7/7 × 3) | Expectations corrected in file (e.g. 22.0, 10.0); no `@broken` — already handled before this pass. |
| `function/call-order.glsl` | **Pass** wasm/rv32c; **rv32n** 5/6 (1 unimplemented) | Not changed this pass. |

## Code touched (this pass)

- `lp-shader/lps-frontend/src/lower_stmt.rs` — `Store` to array local: handle `Expression::CallResult` like stack-to-stack memcpy via `copy_stack_array_slots`.
- `lp-shader/lps-filetests/filetests/function/edge-return-type-match.glsl` — removed `@broken` after fix (`--fix`).

`parse.rs` qualifier normalization was already present on branch; no additional edit required for `edge-const-out-error`.

## Commands run

```bash
# Section A sweep (three targets)
./scripts/glsl-filetests.sh -t wasm.q32,rv32c.q32,rv32n.q32 --concise \
  vec/bvec2/fn-mix.glsl vec/bvec3/fn-mix.glsl vec/bvec4/fn-mix.glsl \
  function/edge-const-out-error.glsl \
  function/edge-lvalue-out.glsl \
  function/edge-return-type-match.glsl \
  function/declare-prototype.glsl \
  function/overload-same-name.glsl \
  function/call-order.glsl

# Strip stale @broken on return-type file
cd lp-shader && cargo run -p lps-filetests-app --bin lps-filetests-app -- \
  test -t wasm.q32,rv32c.q32,rv32n.q32 --fix function/edge-return-type-match.glsl
```

## Blockers / needs user input

- None for this slice; **bvec `mix`** intentionally unsupported pending upstream resolver or a non-Naga strategy outside this roadmap.

## Recommended next autonomous subtask

1. **Deferred milestone:** implement general writable-access resolver for `out`/`inout` (see deferred doc) and unbreak `function/edge-lvalue-out.glsl` + related filetests.
2. **M3 remainder (out of this slice):** `control/ternary/types.glsl` struct row after aggregate phi/copy audit; revisit **rv32n** `call-order` unimplemented line.
3. **Docs:** update `broken.md` Section A table when convenient (optional; not required for this PR).
