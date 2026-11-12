# LPS GLSL Default Closure Notes

Date: 2026-05-13

## Scope

Make `lps-glsl` the normal runtime shader frontend, keep the Naga-backed path as
an explicit opt-in reference feature, record the final parity/size/speed state,
and run a short profiling pass to catch obvious CPU or memory regressions.

## Current State

- `rv32lpn.q32` full filetests pass with expected exclusions:
  `5212/5212 tests passed`, `757/757 files passed`, `11 expected-failure`.
- Texture filetests pass on `rv32lpn.q32`:
  `51/51 tests passed`, `35/35 files passed`.
- Final measured ESP32-C6 app size with the native frontend:
  `2,071,568/3,145,728 bytes`, `65.85%`.
- Final measured `examples/basic` compile time with the native frontend:
  `195ms`.
- Earlier Cargo feature naming treated Naga as the default in several runtime
  crates. The cleanup flips those defaults and uses only the short `naga`
  feature name.
- Compile-mode profile written to:
  `profiles/2026-05-13T09-40-55--examples-basic--compile--lps-glsl-default-compile`
- Profile summary:
  peak tracked live heap about `61.7 KB`, about `266 KB` free at peak, and no
  obvious frontend-specific memory cliff. CPU time is split between native
  backend compile work and `lps-glsl` type checking, with no single trivial
  hotspot.

## Decisions

- Add a short `naga` feature with no legacy compatibility aliases.
- Make `naga` default-off in runtime crates.
- Make `lps-glsl` the implicit default frontend when `naga` is absent.
- Keep filetest reference target `rv32n.q32` as-is for now; filetests still need
  both frontends available.
- Keep bit reinterpret builtins unsupported:
  `floatBitsToInt`, `intBitsToFloat`.
- Keep broad GPU/pipeline-only features out of scope:
  pack/unpack, NaN/inf propagation edge tests, shader-stage IO, buffers,
  `shared`, and related `global-future` tests.
- Consider `frexp` and `modf` later with explicit Q32 semantics.

## Open Follow-Up

- Stale milestone-flavored diagnostics were cleaned up; frontend errors no
  longer mention `M3`.
- Decide whether `lp-cli shader-debug` should grow a `--frontend naga` flag or
  stay native-frontend-only by default.
