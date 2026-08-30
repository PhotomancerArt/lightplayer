---
status: fixed
found: 2026-08-24 # how: browser walk of the lamp-preview voronoi branch (fyeah project preview)
fixed: this change
area: lpc-engine (map2d resolve), everything downstream of ResolvedMappingCompact
class: unit-mismatch
related: [docs/adr/2026-08-24-canvas-object-renderables-and-alignment.md]
---

# map2d resolve passed the doc-space `sample_diameter` through as texture pixels

**Symptom** — After authoring fyeah's truthful `sample_diameter` (2.0 →
26.0 doc units, the physical bulb), the product preview drew a handful of
giant voronoi cells filling the whole box instead of the sign's 231-lamp
mosaic, and the engine's sampling footprint per lamp became ~26 texture
pixels. Before the authoring fix the same mechanism was the fyeah
"engine-preview overlap blur": 2.0 *doc units* (≈ 0.06 texture px after a
honest fit) was read as 2.0 *texture px*, so adjacent lamps 0.5 px apart
sampled overlapping 2 px blobs.

**Root cause** — `mapping_from_map2d_doc` aspect-fits the resolved lamp
*positions* into texture space (`fit_points`) but copied
`doc.sample_diameter` into `ResolvedMappingCompact` unscaled. The
carrier's contract (its own field doc) says texture pixels; every
consumer (`normalized_sample_radius`, the display-layout probe, the
engine sampler) divides by texture dimensions. A doc-space length crossed
a space boundary without the transform its companion positions took.
The old dot renderer's absolute clamps (diameter 3.5–18% of the box, 5 px
floor) masked the mismatch on the display side — one unit-blindness hid
the other; killing the clamps surfaced it in one browser walk.

**Fix** — `fit_scale` in `lpc-mapping` returns the texture-pixels-per-
doc-unit factor of the same fit (`min(tw/bw, th/bh)`, frame-else-bounds),
and the resolve multiplies the diameter by it. The differential test's
slot-form oracle carries the fitted diameter too (the slot form's
`sample_diameter` was always texture px by contract).

**Regression coverage** — `fit_scale_matches_the_position_fit`
(lpc-mapping) pins the factor against the position fit;
`sample_diameter_rides_the_position_fit` (lpc-engine map2d) pins the
end-to-end scaling;
`compact_carrier_matches_the_expanded_slot_form` keeps the two mapping
representations agreeing on the fitted value.

**Lesson** — when a value crosses a coordinate-space boundary, every
*length* in its bundle must take the same transform as the *positions*,
or the unit lie surfaces only when someone renders truthfully. Absolute
render clamps don't just assume a scale — they hide the upstream code
that assumes one too.
