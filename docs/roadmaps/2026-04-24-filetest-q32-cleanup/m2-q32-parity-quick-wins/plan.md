# M2: q32 parity and quick wins — implementation checklist

Execution notes: `docs/roadmaps/2026-04-24-filetest-q32-cleanup/m2-q32-parity-quick-wins.md` and `00-notes.md`. Decisions: `../decisions.md`. Numeric semantics: `docs/design/q32.md`.

## Preconditions

- [x] M1 annotation baseline in place (`@broken` / `@unsupported` as applicable).

## Section B (wasm q32 cast parity)

Only if still failing on current `main` after measurement.

- [x] `scalar/int/from-float.glsl`, `scalar/uint/from-float.glsl`, `vec/uvec2|3|4/from-mixed.glsl`, `scalar/float/from-uint.glsl` — **spot-check: all green on wasm.q32, rv32c.q32, rv32n.q32**.
- [x] Wired `lpvm-wasm` Q32 `FtoiSatS` / `FtoiSatU` lowering through the existing trunc/clamp helpers instead of inline shifts. This also clears stale dead-code warnings in `emit/q32.rs`.

```bash
./scripts/glsl-filetests.sh -t wasm.q32,rv32c.q32,rv32n.q32 --concise \
  scalar/int/from-float.glsl scalar/uint/from-float.glsl \
  vec/uvec2/from-mixed.glsl vec/uvec3/from-mixed.glsl vec/uvec4/from-mixed.glsl \
  scalar/float/from-uint.glsl
```

Validation after wiring wasm helpers:

```bash
./scripts/glsl-filetests.sh -t wasm.q32 --concise \
  scalar/int/from-float.glsl scalar/uint/from-float.glsl \
  vec/uvec2/from-mixed.glsl vec/uvec3/from-mixed.glsl vec/uvec4/from-mixed.glsl
# 60/60 pass

cargo clippy -p lpvm-wasm -- --no-deps -D warnings
# clean
```

## Harness: vector splat in `// run` argument parsing

- [x] Support GLSL-style `vecN(scalar)` (and `ivecN` / `uvecN` / `bvecN`) in `lps-filetests` value parser (`parse_*_vector_constructor` splat branch in `lp-shader/lps-filetests/src/util/value_parse.rs`).
- [x] Unit tests: `cd lp-shader && cargo test -p lps-filetests util::value_parse::tests` (includes `vec4(1.0)` splat, `ivec3`, `bvec2`, `uvec4`, etc.).

## Wrong expectations / doc-aligned fixes (verified on current tree)

All of the following pass on **wasm.q32**, **rv32c.q32**, and **rv32n.q32** with no file edits required in this pass:

- [x] `function/declare-prototype.glsl` — `// run: ... vec4(1.0), vec4(2.0)` works; harness splat + comma splitting in `parse_assert.rs` / `value_parse.rs`. No `@broken` in file.
- [x] `function/param-default-in.glsl` — expectations (including `length` `~=` and `mat2` product) match backends.
- [x] `builtins/matrix-determinant.glsl` — expectations match (incl. `mat4` diagonal det 120).
- [x] `builtins/integer-bitcount.glsl` — int + `ivec2` rows pass; uint rows remain `@unimplemented(jit.q32)` only.
- [x] `builtins/common-roundeven.glsl` — `vec4` row documents tie-to-even for **−0.5 → 0.0** per GLSL / `docs/design/q32.md` §5; no `@broken`.

## Validation (this milestone)

**Do not** use `jit.q32` as a pass/fail target (deprecated). Existing `jit` annotations may stay.

```bash
# Unit tests (harness) — workspace is lp-shader/
cd lp-shader && cargo test -p lps-filetests util::value_parse::tests

# Targeted filetests — patterns are paths under filetests/ (not bare filenames)
./scripts/glsl-filetests.sh -t wasm.q32,rv32c.q32,rv32n.q32 --concise \
  function/declare-prototype.glsl function/param-default-in.glsl \
  builtins/matrix-determinant.glsl builtins/integer-bitcount.glsl \
  builtins/common-roundeven.glsl
```

**Full suite** (`just test-filetests`) is for milestone close-out / CI, not required for every subtask per roadmap notes.

## Blockers / questions for parent

- If Section B (wasm) regresses: reconcile `docs/design/q32.md`, reference `Q32`, and backends before changing emitters; do not guess.
- `function/call-order.glsl`: defer from M2. Current evidence points to an rv32n native memory trap involving calls/globals/stack-frame behavior, not a q32 quick win. Revisit in M5/native-runtime investigation unless a later `DEBUG=1` trace proves a tiny fix.

## Recommended next subtask

- M2 is effectively complete for the safe quick-win scope. Next autonomous work should move to M3/M5 triage or run a broader non-jit validation pass before closing the milestone.
