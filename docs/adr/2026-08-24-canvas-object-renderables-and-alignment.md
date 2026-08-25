# Canvas objects render as outline + lamp polygons, aligned by an authored prop

Date: 2026-08-24
Status: accepted

## Context

Pass 2 of the patching surface (PR #439) made objects on the mapping/
patching canvas clickable THINGS by drawing a padded convex hull per
object. The visual gate rejected that language's limits: convex hulls
swallow concavity (a C-shaped strand's mouth becomes clickable dead air),
uniform tiny lamp dots dissolve at zoom, and nothing captured the physical
fact that a strip often wraps a form. A design spike
(`spikes/canvas-design-language/`, PR #443) converged the replacement over
five gated rounds.

## Decision

1. **Every canvas object provides two derived renderables** (lp2014's
   model): an **outline stroke** and a set of **lamp polygons** (voronoi
   cells). Both are view artifacts computed at the single sprite seam
   (`arrange.rs`) and, dived, per render pass (`layers/bodies.rs`) — never
   stored geometry.
2. **Stroke alignment is authored document data**: map2d format 4 adds
   `align: on | inside | outside` (Illustrator vocabulary) to Path and
   Polygon shapes. It is render-only — the resolver and engine are
   untouched — but it is physical fixture truth (LEDs lining a channel
   letter's inner wall), so it lives in the doc like `reversed`, not in
   editor meta. An older build would silently drop it on round-trip,
   hence the format bump (the format-3 ids precedent).
3. **Outlines are geometric**: polyline offset with round joins and a
   2.5r miter clamp, one loop per strand (a resolver span cut at path
   gaps), fill-rule nonzero. Inside/outside are one-sided bands whose
   edge IS the lamp path. Raster/marching-squares outlines are rejected
   (floating-island and unfilled-gap artifact classes).
4. **No absolute lengths in canvas rendering.** Doc coordinate units are
   arbitrary per document; every render length derives from the doc's own
   numbers — cell radius = 0.92 × the strand's median pitch floored by
   `sample_diameter / 2`; outline reach = 0.65 × pitch floored by
   0.55 × `sample_diameter`. Absolute clamps destroyed the peach (authored
   ~5× coarser) and per-lamp nearest-neighbour radii amplified authoring
   jitter on the fyeah sign; the regression tests pin both with the real
   docs' figures.
5. **Cells are for path-family objects; color is the sampled lamp color.**
   Grid/ring keep dots. Cells carry the same `data-sprite-lamp` live-fill
   hooks the dots carried; in the dived editor cells are context (reduced
   opacity, no live hook — the editing dots stay the live surface).
6. **Selection: the hit body is symmetric and generous, independent of
   the visual alignment** — the on-path band plus ~10 screen px of slop
   (a genuine hit outranks a near-miss; zero slop reproduces the shipped
   rules). Overlap resolution: first click by the v1 rules, a repeat
   press within ~9 px cycles the candidate stack, a ~420 ms hold (mobile
   long-press) or an unmoved right-click opens a candidate menu;
   right-drag remains the pan.

## Alternatives rejected

Beam-wedge lamp fill (not solid); per-object hue palettes at rest and
name chips (deferred — noise at dome scale, violet collides with the
bound=violet convention); marching-squares outlines; convex hulls
(superseded — `hull.rs` deleted with its 4×r miter fling).

## Consequences

- `lpc-mapping` format history gains format 4; pre-format-4 docs are
  byte-identical (align skipped when `on`).
- Both canvas views and the dived editor speak one body language; the
  project/output raster previews do not yet (follow-up chip).
- Deferred: authored/custom outlines (the channel-letter full face),
  fusing strand-runs whose gap is below a threshold (dome cluster
  cohesion), preview-painter cells.
