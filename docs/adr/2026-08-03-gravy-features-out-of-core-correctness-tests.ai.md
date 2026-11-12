# ADR: Gravy features stay out of core-pipeline correctness tests

- **Status:** Accepted
- **Date:** 2026-08-03
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The render path carries two *quality enhancement* features in
`DisplayPipeline`: temporal dithering (a per-LED error carry that recovers
sub-count precision across frames) and frame interpolation (prev/current/next
buffers blended across the frame interval). Both are **gravy**: extras that
can improve output quality on top of a correct core pipeline, not part of the
pipeline's correctness. At realistic channel sizes they often cannot even
help — at 300 LEDs per WS281x channel the frame rate is too low for temporal
dithering to average into anything.

Both features are *temporal*: their output depends on state accumulated from
previous frames. That property makes them poison inside a differential
correctness test of the core pipeline. A test that asserts "path A's output
is bit-identical to path B's" — the pattern this repo relies on for value-path
changes (PR #285, the Q32/f32 oracle walks, the classic bit-exactness
criterion) — stops testing the core the moment dither carry or interpolation
phase is inside the comparison: any legitimate difference in *when* frames
were produced smears into a byte diff, and the test fails on timing rather
than on values. The M4 memory-pressure work made this concrete: dropping and
lazily rebuilding per-LED state during a shader compile necessarily discards
dither carry, and a compile takes up to ~100 ms in which dropping a frame or
ten is expected and fine. A bit-identity invariant that includes temporal
state would forbid a reclaim that is visually free (WS281x LEDs latch their
last colour while state is dropped).

## Decision

**Correctness tests of the core pipeline run with gravy features disabled or
their state exempted from comparison.**

- Core-pipeline differential tests (drop/rebuild identity, backend oracles,
  bit-exactness proofs) run with **interpolation off** and compare
  steady-state rendered output; **dither carry and other display-pipeline
  temporal state are outside the comparison**.
- Gravy features get their **own dedicated correctness tests** — testing
  *their actual correctness* is the one place they belong in a test (the
  pattern: `conditional_buffers_are_bit_identical_to_three_buffer_reference`
  in `lp-core/lpc-shared/src/display_pipeline/pipeline.rs`).
- Runtime contracts (e.g. the M4 memory-pressure contract) give temporal
  gravy state **no preservation guarantee**: it may be dropped and
  reinitialized at safe points; frame continuity across such a point is not
  required.

"Gravy" here means: a feature that enhances output quality but whose absence
still yields correct output — today dither and interpolation; the same test
posture applies to future features of that class.

## Consequences

- The M4 drop → rebuild differential tests assert bit-identity of the core
  mapping/sample/render path only, with interpolation off. They do not assert
  frame-by-frame identity through a compile transient.
- Memory-pressure implementations may drop `DisplayPipeline` buffers
  (including dither carry) without violating any test or contract.
- A future test that wants to include a gravy feature in a cross-path
  comparison must first pin that feature's own behaviour in a dedicated test,
  and must control its temporal state explicitly (same warm-up on both
  paths), not rely on incidental determinism.
- Reviewers can reject "the differential test needed interpolation enabled to
  pass" as a defect signal, not a test configuration choice.
