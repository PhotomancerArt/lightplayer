# M1: Annotation baseline — implementation checklist

## Goal

- Mark intrinsically q32-excluded tests with `@unsupported(wasm|rv32c|rv32n).q32` (no new `jit.q32` markers).
- Mark intended q32 behavior that still fails with `@broken(wasm|rv32c|rv32n).q32` (target-narrow if failures are target-specific; default matrix is rv32n + rv32c + wasm).
- Per-line `// run:` classification for mixed files (notably `common-intbitstofloat.glsl`).
- Keep `global-future/*` out of the “broken fix” backlog (leave `@unimplemented` there).
- BVec `mix` ambiguity: prefer `@unsupported` + short comment (no Naga patch in M1).

## Triage source

- [unsupported.md](/docs/reports/2026-04-23-filetest-triage/unsupported.md)
- [broken.md](/docs/reports/2026-04-23-filetest-triage/broken.md) (2026-04-23 snapshot — spot-check when marking `@broken`).

## Checklist

### Unsupported corpus (`@unsupported`)

- [x] Bit reinterpret / no real f32: `builtins/common-floatbitstoint.glsl` (all `// run:` rows — whole-TU skip when every row unsupported).
- [x] Pack/unpack double, half, unorm: `pack-*.glsl`, `unpack-*.glsl` in `builtins/`.
- [x] `common-frexp.glsl`, `common-modf.glsl`.
- [x] `common-intbitstofloat.glsl` — **only** the Inf/NaN-class rows (IEEE / non-finite expected); literal-width / const-eval rows use `@broken` instead. *File comment added at top.*
- [x] Edge / NaN / Inf / trig domain files: ensure `@unsupported` wins (no redundant `@unimplemented` before `@unsupported` on the same `// run:`). *`edge-trig-domain.glsl` first `// run:` block deduplicated (2026-04-25).*
- [x] BVec `vec/bvec{2,3,4}/fn-mix.glsl`: `@unsupported` + resolver ambiguity note; drop duplicate `@unimplemented(jit)`.

### Broken corpus (`@broken`)

- [x] Remaining `// @unimplemented` blocks that represent **intended** q32 behavior (per `broken.md`, excluding `global-future/*`) → `@broken` on the three default-validation targets. *Most of the tree already used `@broken(…)`; `vec/uvec3/from-uvec.glsl` was migrated from a 4× `@unimplemented` block via mechanical replace.*
- [x] Re-verify target-only failures (e.g. old `broken.md` §B wasm-only); narrow markers if the current tree no longer fails on rv32. **Spot-check: `scalar/*`, `vec/uvec*/from-mixed` pass on wasm/rv32 — no change.**
- [x] `function/param-default-in.glsl`: standalone `@unimplemented(jit.q32)` removed (all cases pass on default `wasm`/`rv32` matrix; default pipeline does not use JIT for acceptance).

### Excluded from `@broken` backlog

- [x] `global-future/*` — no change to “broken” semantics; remains future surface / compile-error harness.

## Validation (commands)

| Command | Outcome (recorded) |
|--------|---------------------|
| `scripts/glsl-filetests.sh --concise` | **Pre-M1 (2026-04-25):** exit 0, 14873 pass / 819 expected-failure, ~3m40s. Re-run after local edits. |
| `scripts/glsl-filetests.sh --target wasm.q32 --concise builtins/edge-trig-domain.glsl function/param-default-in.glsl` | **Post-edit:** exit 0; edge-trig 10 unsupported; param-default 10 pass. |
| `scripts/glsl-filetests.sh --target wasm.q32 --concise builtins/common-intbitstofloat.glsl` | 1 pass, 3 unsupported, 4 expected-fail (`@broken` rows; harness still labels bucket “unimpl” in summary). |
| `scripts/glsl-filetests.sh --target wasm.q32 builtins/common-intbitstofloat.glsl` | Exit 0 after edits; 1 pass / 4 expected-failure / 3 unsupported. |
| `scripts/glsl-filetests.sh --target rv32n.q32 function/call-order.glsl` | Exit 0 after edits; 5 pass / 1 expected-failure. |
| `scripts/glsl-filetests.sh --target wasm.q32 vec/bvec2/fn-mix.glsl` | Exit 0 after edits; 6 unsupported. |

**Note:** `just test-filetests` in this repo runs `scripts/glsl-filetests.sh` once (default targets per `lps-filetests` README: rv32n + rv32c + wasm).

## Open questions / follow-ups

- `vmcontext/fuel-read.glsl` has a one-off `// @unimplemented(jit.q32): …` with trailing prose — not converted in M1; revisit if the harness should support inline notes.
- If someone runs `--target jit.q32` after dropping `jit` from new markers, some tests may no longer be skipped on JIT; default CI path does not include JIT in `just test-filetests`.

## Recommended next autonomous subtask

M2 of the filetest Q32 cleanup roadmap: q32 parity quick wins (e.g. wasm scalar cast alignment if any regress, or the next triaged bucket).
