# M1 Annotation Baseline — Notes

## Goal

Re-run the triage corpus, mark intrinsic q32 exclusions as
`@unsupported`, mark intended-but-failing q32 behavior as `@broken`,
and establish the filetest baseline for later milestones.

## Current Findings

- M1 primarily touches `lp-shader/lps-filetests/filetests/**/*.glsl`
  for every current file listed in `unsupported.md` and `broken.md`.
- Unsupported file groups from `unsupported.md`:
  - `builtins/common-floatbitstoint.glsl`
  - `builtins/common-intbitstofloat.glsl` (split IEEE/Inf/NaN rows
    from the literal-width rows tracked as broken)
  - `builtins/pack-double.glsl`, `builtins/unpack-double.glsl`
  - `builtins/pack-half.glsl`, `builtins/unpack-half.glsl`
  - `builtins/pack-unorm.glsl`, `builtins/unpack-unorm.glsl`
  - `builtins/common-frexp.glsl`, `builtins/common-modf.glsl`
  - `builtins/edge-exp-domain.glsl`,
    `builtins/edge-nan-inf-propagation.glsl`,
    `builtins/edge-trig-domain.glsl`
- Broken file groups follow `broken.md` Sections A-G, but should be
  refreshed against the current tree before marking because the report
  is a 2026-04-23 snapshot.
- `global-future/*.glsl` is explicitly out of the broken backlog.
- Annotation plumbing already exists:
  `lps-filetests/src/parse/parse_annotation.rs`,
  `lps-filetests/src/targets/mod.rs`, and test-run accounting support
  `@unsupported` as skip and `@broken` as expected failure.
- Canonical target strings are `wasm.q32`, `jit.q32`, `rv32c.q32`,
  and `rv32n.q32`.

## Questions For User

- For files with mixed rows, should M1 always do line-by-line
  `@unsupported` vs `@broken` classification rather than marking whole
  files? **Answered:** Yes, classify per `// run` line.
- For failures that are wasm-only in the refreshed current tree, should
  M1 prefer backend-scoped `@broken(wasm.q32)` or mark all q32 targets
  if the report suggested a broader root cause? **Answered:** Yes,
  prefer the current observed target shape.
- Should M1 use the existing `lps-filetests-app --mark-broken` /
  `--mark-unsupported` automation where possible, or hand-edit markers
  for more reviewable diffs?

## Implementation Notes

- Start from `docs/reports/2026-04-23-filetest-triage/unsupported.md`
  and `broken.md`, but re-run relevant files before applying markers.
- Intrinsic no-real-f32 exclusions should be marked across all q32
  targets.
- Prefer per-line annotations. Some files, especially
  `common-intbitstofloat.glsl`, contain both unsupported and broken
  rows.
- Avoid duplicating existing `@unsupported` markers in already annotated
  edge/builtin files.
- `global-future/*` should be documented as future product surface if
  it appears in validation output, not folded into the q32 broken list.

## Validation

- Targeted `scripts/glsl-filetests.sh --target <target> <file>` runs.
- Use `LP_FILETESTS_THREADS=1` for targeted `jit.q32` runs if needed.
- Final `just test-filetests`.
