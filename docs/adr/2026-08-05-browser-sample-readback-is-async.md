# ADR: Browser GPU sample readback is async — one frame of latency, black first frame

- **Status:** Accepted
- **Date:** 2026-08-05
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Related:** `2026-07-09-preview-fidelity-tiers.md`,
  `2026-07-09-gpu-path-forks-at-glsl.md`,
  `../debt/gpu-tier-cannot-sample-led-output.md`,
  `../defects/2026-07-28-tick-error-restated-every-frame.md`

## Context

The fidelity-tiers ADR grants a browser runtime the GPU tier on one
question — is WebGPU available in this worker? — and never revisits it.
Fixture control rendering samples the visual at each LED's point
(`sample_rgba16`), which on wgpu needs a GPU→CPU readback, and the only
readback `lp-gfx-wgpu` had was the blocking buffer map in `read_back` —
native-only, because a browser cannot block on `map_async`. So a
GPU-tier browser runtime answered every `sample_rgba16` with an error,
and a project containing a fixture node — most real projects — failed
every frame forever: `LpServer::tick` logged the error each tick (512+
consecutive observed at the honest-device-preview G1 walk 2026-08-05),
and every UI surface asking the preview host for lamps (module face
control hero, Control Output produced-product rows,
`ControlProductPreview`) sat "not tracked" indefinitely. The condition
was carried as `docs/debt/gpu-tier-cannot-sample-led-output.md`; its
exit options were capability-aware tier selection (hands most projects
back to the CPU tier), skipping control rendering in previews (a
product decision about what a preview is for), or making the sample
path work asynchronously.

## Decision

**The browser GPU tier implements `sample_rgba16` as a one-frame-latency
`map_async` pipeline.** Each call submits the sample draw as before,
then instead of blocking: it harvests the previous call's readback if
its map has resolved (the worker's event loop turns between ticks, so
in practice each map lands before the next tick), issues a
copy-and-map for the frame just drawn whenever the persistent `MAP_READ`
buffer is free, and serves the most recent completed frame. The first
frame — and any frame whose map is still in flight — serves the last
completed results, which at startup means black. Native keeps the
blocking, same-frame readback unchanged.

Consequences of the shape:

- **Latency, not staleness-unbounded:** results trail by exactly one
  frame in the steady state; a slow map only widens the window until it
  resolves (no queue of readbacks builds up — at most one is in
  flight).
- **Quantization is shared:** both targets quantize with the same CPU
  packing rule (`quantize_unorm16`), so a sampled lamp value on the
  browser GPU tier matches native for the same rendered frame.
- **The tiers ADR's no-silent-fallback rule is respected:** nothing
  falls back to the CPU tier; the GPU tier now genuinely supports the
  capability, off by one frame.

## Consequences

- Fixture-bearing projects render on the browser GPU tier; the
  per-tick `sample_rgba16` error and the permanently-pending lamp
  surfaces are gone.
- Control products derived from sampling lag the visual by one frame in
  browser previews. At preview frame rates this is imperceptible; it is
  documented here so a future exact-sync consumer knows to look.
- `read_back` (whole-texture) remains native-only; the sample pass owns
  its own async pipeline. A browser consumer needing whole-texture
  bytes still has no path — that gap is unchanged and intentional.
- The sample pass and its GLSL sample-unit assembly now compile on
  wasm32 (previously `cfg`'d out), so sample-unit compile errors can
  now surface in the browser too — same surface as native.

## Alternatives considered

- **Capability-aware tier selection** (debt exit 1): honest but hands
  most real projects back to the CPU tier — close to deleting the GPU
  preview.
- **Skip control rendering in previews** (debt exit 2): plausible
  product call, but the honest-device-preview work made lamps a
  first-class preview surface — previews now *want* control products,
  so "a preview only wants the visual" no longer holds.
- **Blocking on the map via busy-wait/atomics.wait:** unavailable on
  the main-thread-adjacent worker; WebGPU maps resolve only when the
  event loop turns.
- **Classify the failure once and stop retrying** (the minimum): stops
  the log noise but leaves the capability gap; rejected in favor of
  fixing the gap.

## Follow-ups

- The gallery gpu-tier badge could disclose "lamps trail by one frame"
  if anyone ever asks; not worth UI surface today.
