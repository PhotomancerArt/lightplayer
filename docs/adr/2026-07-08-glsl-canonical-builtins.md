# ADR: GLSL is the canonical source of truth for lpfn builtin semantics

- **Status:** Accepted (user review 2026-07-08, GPU preview roadmap M2 review gate)
- **Date:** 2026-07-08
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The lpfn builtin library (noise, random, color conversion, saturate) has
historically been defined by its handwritten Rust Q32 implementations in
`lp-shader/lps-builtins/src/builtins/lpfn/` — structural rewrites of
lygia/stegu GLSL originals using integer hashing (`lpfn_hash`), Q16.16
gradient LUTs, and wrapping/i64-staged fixed-point arithmetic. The `_f32`
Rust variants turned out to be stubs that convert to Q32 and call the Q32
implementation; they carry no independent float semantics.

The GPU preview path (GPU preview shader abstraction roadmap, 2026-07-08)
needs the builtins as **GLSL source** to splice into shaders as a prelude
before naga translates them to WGSL. Preview fidelity requires the GPU to
render the *same fields* the device renders, so the GLSL sources must
express LightPlayer's actual algorithms (integer hash + LUT structure), not
the lygia originals they were ported from.

## Decision

1. **Canonical GLSL sources define lpfn builtin semantics** at ideal (f32)
   precision. They live in `lp-shader/lps-builtins/glsl/lpfn/`, mirror the
   Rust concept-per-file layout, and are embedded via the
   `lps_builtins::canonical_glsl` manifest. Each source is a float+integer
   GLSL port of the LightPlayer algorithm as implemented in the Q32 Rust
   sources (same integer hashes, same structure, ideal-precision constants),
   with lygia/stegu/noiz attribution headers where the lineage warrants it.
2. **The Rust Q32 implementations are device approximations** of the
   canonical semantics. They are bound by the conformance suite in
   `lp-shader/lps-filetests/src/conformance/`, which compares them against a
   **float oracle** — the canonical GLSL compiled by `lps-frontend` and
   executed by the native-f32 LPIR interpreter — within per-builtin
   documented tolerances:
   - *Pointwise* absolute tolerances for builtins whose randomness routes
     through exact integer hashing (snoise, worley, psrdnoise, fbm-on-snoise,
     saturate, color conversions).
   - *Statistical* conformance (range, mean, seed sensitivity) for the
     chaotic sin-hash family (random, srandom, and gnoise/fbm3_tile built on
     them): `fract(sin(x) * 43758.5453)` amplifies any finite-precision
     representation difference into a full wrap, so pointwise agreement
     between Q16.16 and f32 is mathematically impossible for them.
3. **The Rust `_f32` stubs are legacy.** They are not a reference for
   anything; canonical-source validity is instead established by
   line-by-line porting review, closed-form reference-formula tests, and
   continuity/property tests on the oracle. Retiring the stubs is future
   work.
4. **Q32 numeric semantics are unchanged** — `docs/design/q32.md` remains
   normative for how Q16.16 arithmetic behaves. This ADR adds a float
   reference layer *above* it that defines what the builtins compute.

## Consequences

- The GPU preview prelude (roadmap M3) and the device path share one
  algorithm definition; visual divergence is bounded by the conformance
  tolerances plus f32-vs-Q32 arithmetic drift in user shader code.
- Q32 implementation bugs become *detectable*: the M2 conformance run
  immediately found that `psrdnoise3_q32` inverts the simplex rank-order
  `step()` (wrong corners, dead zones returning exactly 0). The failing
  comparison is annotated `known_q32_bug` (expect-fail) in the suite; the
  fix is follow-up work.
- Canonical sources use the real `lpfn_*` names for M3 splicing. Because
  `lps-frontend` reserves the `lpfn_` prefix for builtin imports, harnesses
  compiling the canonicals through the normal frontend rename the prefix
  (`lpfn_` → `lpo_`) first.
- New/changed builtins must land with a canonical `.glsl`, a conformance
  spec (tolerance + corpus), and the usual Q32 implementation.

## Alternatives Considered

- **Lygia originals as canonical.** Rejected: the Q32 ports deviate
  structurally (integer hash vs mod-289 float permute); GPU previews would
  render different noise fields than the device, and conformance would
  measure algorithm mismatch instead of precision error.
- **Q32-compiled canonical GLSL as the oracle.** Rejected as the general
  mechanism: the sin-hash multiplier (43758.5453) exceeds Q16.16's ±32768,
  mod-289 float permute intermediates (~2.8M) overflow it, and handwritten
  impls rely on i64 staging/wrapping ops that naive compilation lacks. It
  survives for the simple builtins and is kept as an annotated optional
  tier (color/math conversions, essentially bit-exact).
- **Rust `_f32` implementations as canonical.** Rejected: they are Q32
  stubs; and Rust cannot be spliced into GPU shaders.

## Follow-ups

- Fix `psrdnoise3_q32`'s inverted rank-order step (and audit other
  `Vec3Q32::step` call sites for the swapped-argument convention).
- Random-family constant drift: several Q32 constants do not match their
  documented values (e.g. `random2_q32` `DOT_X` = 12.9922 vs documented
  12.9898; `RANDOM_MULT` ≈ 43756.17 vs 43758.5453). Statistically
  irrelevant, but worth reconciling when the family is next touched.
- Adjacent seeds (delta < ~10) can produce identical sin-hash fields on the
  device (seed enters as a 2^-16-radian phase step below Q32 sine
  granularity). Decide whether seed should be premixed (e.g. hashed) before
  entering the angle.
- Retire the `_f32` stub implementations.
- Teach the wasm.q32 backend to accept overloaded local GLSL functions
  (duplicate export names currently fail wasm validation), then drop the
  overload-uniquifying transform in the Q32-compiled conformance tier.
