# `filled_polygon`: the shaped-matrix primitive, with a derived lamp count

Date: 2026-08-26
Status: accepted

## Context

Shaped matrices are endemic in this niche. A custom PCB cut to a
silhouette — a triangle, a hex, a logo mark — populated with a lattice of
LEDs at a fixed pitch is what people actually fabricate, and the mapping
document had no way to say it. The workarounds in the neighbouring tools
are the tell: WLED asks for a bounding grid plus a hand-maintained gap
list, and Pixelblaze asks for a literal array of coordinates. Both make
the author carry a lamp count that the geometry already implies, and both
make editing the silhouette a re-derivation chore rather than a drag.

The `logo-sign` example (the brand artifact this plan builds around) is
exactly that shape: a triangular PCB matrix at ~16-unit pitch, ~130 LEDs.
Authoring it as a `grid` plus a mask, or as a baked `path`, would have made
the artifact unmaintainable and taught the wrong thing about the format.

`Map2dShape` already had a `Polygon` (format 3): a closed outline with
`count` lamps distributed along its **perimeter**. A filled polygon shares
only the word.

Vision and rulings: `2026-08-24-1720-brand-example-masked-grid/`
(D5–D11, Q5).

## Decision

1. **A new externally-tagged variant, `Map2dShape::FilledPolygon`**
   (format 5) — not a mask on `GridShape`, and not a mode flag on
   `PolygonShape`.

   Against mask-on-grid: a grid is a rectangle of `cols × rows`, and its
   count *is* that product. Bolting an outline onto it would make `cols`
   and `rows` a bounding box nobody authored and break the one thing a
   grid promises. `GridShape` stays untouched (D10).

   Against a mode flag on `Polygon`: the field sets are near-disjoint.
   A perimeter polygon has `count` and `align`; a filled one has `pitch`,
   `angle_deg`, `origin`, `routing`, `start_corner`, and no count at all.
   One struct holding both would have every field ignored half the time.
   **Tool ≠ type**: the editor offers one Polygon tool with an
   Outline/Filled switch, and the switch converts between two honest
   types rather than setting a bit on one dishonest one.

2. **The count is derived, and this is the first shape where that is
   true.** A cell is populated iff its center falls inside the outline;
   routing walks the populated cells row by row, so data order stays
   wiring order. There is no authored number to keep in sync with the
   geometry, and therefore none to get wrong: editing the outline
   re-derives the count.

3. **Derived-count-first means one walk, not two derivations.**
   `filled_polygon_cells` is shared verbatim by `resolve` and
   `shape_lamp_count`. Every other shape can afford a parallel arithmetic
   mirror because an authored `count` cross-checks it; a derived count has
   no such witness, so agreement has to come from there being a single
   code path. The existing mirror-pin test
   (`shape_lamp_count_matches_the_resolver_for_every_kind`) covers the new
   variant, including nested in a repeat and with a turned lattice.

4. **ε-inset inclusion is the determinism rule** (D11): a center counts
   iff it is inside the outline (even-odd ray cast) **and** at least
   `ε = pitch × 1e-3` from every outline segment. This does two jobs with
   one number. It is the tie-break — a center exactly on an edge is
   *always* excluded, so the count never depends on which side a float
   rounds to, which is what makes the derived count safe to build patch
   spans on. And it is the drag damper — while a vertex is being dragged,
   cells near an edge cross a band rather than a line, so the live lattice
   preview does not blink.

5. **The lattice is laid out in a rotated frame anchored to the outline.**
   `angle_deg` turns the lattice, not the outline, about the outline's
   bbox center, via the shared `Rotation2d` — the same bit-exact rotation
   the resolver uses for repeats, so an editor previewing the lattice
   lands on the resolver's floats instead of its own re-derived trig.
   Cell centers sit at half-pitch offsets from the frame bbox minimum, so
   a pitch-aligned outline populates symmetrically; `origin` slides the
   phase for the cases where the board wants it elsewhere.

6. **Routing mirrors `resolve_grid`, over populated cells only** —
   `start_corner` flips rows and columns, `Snake` alternates — with one
   difference recorded on purpose: **snake parity counts visited rows**.
   A row the outline skips entirely is not a row the chain ever reaches,
   so it does not flip the next row's direction. A serpentine chain on a
   shaped board reverses at the end of each row of copper.

7. **Format 5, additively, with the loud refusal the format history is
   built on.** An unknown shape variant cannot be ignored — the document
   would silently lose every lamp the matrix carries — so a document
   using one stamps 5 and older builds refuse it whole. Every optional
   field is `skip_serializing_if` at its default, so a minimal shaped
   matrix serializes as `points` + `pitch`. Note the asymmetry with
   `GridShape`, which has always written its `routing`/`start_corner`
   defaults: existing documents' bytes are untouchable, but a brand-new
   variant gets to start minimal. A test pins both sides.

8. **Stride is 1, and there is no `align`.** A shaped matrix's rows are
   whatever the outline makes them — a triangle's rows grow one cell at a
   time — so no single number is an honest rotation period, the way
   `cols` is for a grid. Override territory, like a non-divisible polygon.
   And stroke alignment describes where a strip sits relative to a line it
   traces; a field of lamps traces nothing.

## Consequences

- `MAP2D_FORMAT` is 5. Existing documents are byte-identical (the CI guard
  `checked_in_examples_rewrite_byte_identically` is untouched); the
  format-refusal test that used `format: 5` as its refused-newer case
  moves to 6.
- Canvas surfaces treat a filled polygon as a **field**, like a grid or
  ring — never a swept ribbon along the chain. Giving it the
  closed-outline treatment its outline suggests would draw the serpentine
  path through the lattice rather than the shape.
  *(Amended at the G1 gate, 2026-08-27, after seeing it on the canvas:
  the band was drawn once and read exactly as predicted — snaking through
  the lattice and overlapping itself — so it is gone. Two things replace
  it. The lamps wear voronoi **cells**, seeded from the field and computed
  by the same `point_cells` the node view's output preview calls, so the
  editor and the preview draw one geometry. And the authored outline is
  drawn as an always-on **silhouette** rather than left to future work:
  it is the board, and a shaped matrix whose shape is invisible until
  selected is a poor teacher. Only the swept body was rejected.)*
- A filled polygon under a repeat expands to baked geometry for now. It
  carries an `angle_deg`, but its lattice anchors to the bbox of the
  rotated outline, so turning it parametrically needs an argument about
  how that anchor moves — deferred to the editor phases.
- Nothing bounds the derived count in the resolver. A pitch far finer than
  the outline yields a very large lattice; the editor's sanitize floors
  `pitch` at 0.5 the way it does for a grid, and a hand-authored document
  remains the author's business (the `MAX_REPEAT_COUNT` precedent).

## Alternatives rejected

A mask/exclusion list on `GridShape` (WLED's shape); a `filled: bool` or
`population: outline | filled` flag on `PolygonShape`; an authored `count`
with the lattice fitted to match it (re-introduces the sync problem the
derived count exists to remove); resolving inclusion by area coverage
rather than center-in-outline (no tie-break, and no cheap answer for a
concave outline).

## Follow-ups

Explicitly left open, all composing against an explicit `pitch` +
`angle_deg`: **holes** (interior rings excluded from population),
**per-row overrides** (a row shifted or shortened for a connector), and
**explicit chain order** (a routing that is neither raster nor serpentine).
Also deferred: the parametric repeat expansion above, and a real body
outline for shaped matrices on the canvas.

## Lineage

Extends `2026-07-27-map2d-document-architecture` (the format-gate posture)
and `2026-08-24-canvas-object-renderables-and-alignment` (why `align` is
document data, and why this shape does not carry it).
