# Milestone 6: Render products (speculative)

## Goal

Explore and implement the next render abstraction after the core engine cutover:
render products that can support both full texture rendering and on-demand
sampling.

This milestone is intentionally speculative. Keep it as a pointer to likely
next work, not a fixed contract.

## Context

Profiling shows the legacy texture -> fixture sampling path is expensive on
ESP32. Texture-backed rendering is useful for debugging and GPU-oriented
systems, but it can waste work when fixtures only need sparse sample points.

The runtime-core design uses "render product" as the produced value concept.
Earlier milestones may keep the only concrete product texture-backed. M6 is
where that abstraction can become real.

## Possible scope

- Define a first-class render product trait/enum.
- Support texture-backed products for compatibility and debugging.
- Support point-sampled or batch-sampled products for fixture-driven demand.
- Let simple shader visuals execute directly at fixture sample points.
- Preserve a full-texture path for debugging, previews, or GPU-oriented hosts.
- Decide how products carry versions via `Versioned<T>` or an engine wrapper.
- Decide how products interact with bus/node-output resolution.

## Open questions

1. Is a render product a value, a trait object, an artifact-owned capability, or
   a node-owned private cache surfaced through the engine?
2. Do fixtures ask for a product and sample it, or ask the engine for a sampled
   fixture-ready batch?
3. How do stack/blur/feedback visuals express dependencies when a full texture
   may be required?
4. How do we keep debugging easy when no full texture is generated?
5. Does the sampled path change shader ABI/codegen, or can it reuse the same
   shader entry point with different coordinates?

## Out of scope until this milestone is planned

- Reworking the core engine scheduler.
- Retiring texture-backed rendering entirely.
- Assuming GPU behavior is the default target.
- Requiring all visuals to support point sampling.

## Suggested plan location

If this remains the right next step after M5, expand this milestone with
`/plan` or `/plan-small` at:

`docs/roadmaps/2026-05-01-runtime-core/m6-render-products/`

## Success criteria

To be decided during planning. Likely criteria:

- Existing texture-backed behavior remains available.
- A simple shader visual can be sampled directly for fixture points.
- The sampled path avoids generating unused texels on ESP32.
- Debug/full-texture fallback remains available.

