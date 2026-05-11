# M4 Matrix Core — plan & status (q32: wasm, rv32c, rv32n)

**Conventions:** GLSL **column-major** is source of truth. **JIT** is deprecated for validation; filetests for product targets are **wasm.q32**, **rv32c.q32**, **rv32n.q32** only. Do not change `// run:` expectations without independent GLSL verification.

## Checklist (scoped safe work from Milestone 4)

- [x] **Doc:** This plan file + alignment with `m4-matrix-core.md` and `00-notes.md`
- [x] **Run targeted filetests** — `matrix/` and `builtins/matrix-*.glsl` (see command below)
- [x] **Annotation hygiene** — no stale `@broken` / `@unimplemented` for the three product targets in matrix groups (see results); `matrix-determinant.glsl` still has `// @unimplemented(jit.q32)` only (left as-is)
- [ ] **Remaining product gaps** — `builtins/matrix-compmult.glsl`: all lines **@unsupported** (out of core matmul M4 scope; not a q32 “paper over” item)
- [ ] **Optional deeper work** — any future matmul/associativity edge that regresses: fix in `lps-frontend` matrix lowering with explicit shared layout helper (`lower_matrix.rs` / `lower_expr.rs`)

## Targeted filetest command (2026-04-24)

Shorthand (same q32 triple):

```bash
./scripts/glsl-filetests.sh -t wasm,rv32c,rv32n --concise \
  matrix/ 'builtins/matrix-*.glsl'
```

Explicit canonical target names (equivalent for current `ALL_TARGETS`):

```bash
./scripts/glsl-filetests.sh --target wasm.q32,rv32c.q32,rv32n.q32 --concise \
  matrix/ builtins/matrix-determinant.glsl builtins/matrix-inverse.glsl \
  builtins/matrix-outerproduct.glsl builtins/matrix-transpose.glsl
```

**Outcome (2026-04-24, this tree):** exit **0**; **1980/1980** non-unsupported tests passed on the matrix + matrix-builtin slice (**660** per target × **3**); **17** lines per target remain **@unsupported** in `builtins/matrix-compmult.glsl` (counted in runner stats); **73/73** files in scope. A first parallel run of the same slice once reported transient “unexpected” rows on a single file—re-run matched **full pass**; use **single target** or **re-run** if diagnosing flakes.

| Area | wasm.q32 | rv32c.q32 | rv32n.q32 | Notes |
|------|----------|-----------|-----------|--------|
| `matrix/*` (all 68 files) | pass | pass | pass | Includes `op-multiply`, compound assigns, constructors, in-file `transpose` tests |
| `builtins/matrix-determinant.glsl` | pass | pass | pass | Jit-only `@unimplemented` line retained |
| `builtins/matrix-inverse.glsl` | pass | pass | pass | 16/16 `// run:` |
| `builtins/matrix-outerproduct.glsl` | pass | pass | pass | |
| `builtins/matrix-transpose.glsl` | pass | pass | pass | |
| `builtins/matrix-compmult.glsl` | 0/17 (unsupported) | same | same | Intentional unsupported surface |

## Triage D vs current tree

[Section D in `docs/reports/2026-04-23-filetest-triage/broken.md`](../../reports/2026-04-23-filetest-triage/broken.md) described widespread matrix failures in an **earlier full-suite snapshot** (2026-04-23). On the **current** tree, the **matrix + matrix-builtin** slice above is **green** for all three q32 product targets, so the remaining M4 work shifts to **sustaining** that parity, `matrix-compmult` if promoted from unsupported, and **not** editing expectations without spec-backed verification.

## `--fix` sweep

`--fix` removes `@unimplemented` / `@broken` for targets when tests **unexpectedly pass**. After grep for product-target annotations in `matrix/` (none) and `builtins/matrix-*.glsl` (jit-only in determinant), a fix sweep is a **no-op** for wasm/rv32c/rv32n; run when annotations exist:

`./scripts/glsl-filetests.sh -t wasm,rv32c,rv32n --fix --assume-yes matrix/ 'builtins/matrix-*.glsl'`

A sweep with `--fix --assume-yes` on the explicit four builtin paths + `matrix/` applied **zero** file edits (no unexpected-pass lines for the three product targets).

**Host validation:** `cargo test -p lps-filetests --lib` — **97** unit tests passed (sanity after touching this plan / runner usage).

## Blockers (none for this scoped pass)

- No **ambiguous** column-major / matmul change was applied; the tree already matches the matrix filetests for the three targets on this slice.
- **matrix-compmult** remains **unsupported** by design in the test file — not a “small localized fix” without a product decision.

## Recommended next subtask

1. **M5+ / separate milestone:** Decide whether **component-wise matrix multiply** (`matrix-compmult`) is in product scope; if yes, implement lowering + ungate `builtins/matrix-compmult.glsl` line-by-line with verified expectations.
2. **Broader M4 validation:** `just test-filetests` with `-t wasm,rv32c,rv32n` (full suite) on occasion to catch **non-matrix** regressions; keep JIT annotations untouched unless a dedicated JIT effort lands.
