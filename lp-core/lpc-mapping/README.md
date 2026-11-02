# lpc-mapping

The LED fixture mapping domain, shared by the engine, the device and Studio:
**mapping = layout + patching**. A fixture's mapping says where each lamp is
and where on the hardware its data goes.

The mapping rebuild (planning `lp2025/2026-10-27-0114-mapping-rebuild`;
ADR `docs/adr/2026-10-27-mapping-model-parts-segments-patcher.md`):

- **The mapping model and map2d format 6** (`part`, `patching`, `map2d6`):
  parts, segments and the patcher, in one file. Built in M1; since M3
  (project format 12) the engine runs it for every fixture, and its patching
  is the engine's one patching path. Studio's Graph edits it too.
- **Edits** (`edit`, behind the `edit` feature): the part-tree edits an
  editor makes. Below, "The `edit` feature".
- **The older map2d document** (formats 1–5, `Map2dDoc`): its resolver is
  also the new model's geometry (each shape's lamps come from
  `resolve_into` on a one-object document). Nothing that runs a project
  reads it: the engine reads format 6 only. What still reads it: the
  logo-sign and small-dome generators (`lpa-studio-web`'s
  `logo_sign_gen.rs`, `tools/lpt-geodome`) build their geometry with it
  before they write format 6; Studio's fixture-face stories draw the
  `corpus` scenes; and the format-12 upgrade's statements
  (`lpa-upgrade/tests/step_v11_to_v12.rs`) check lamps against its
  resolver. Studio's Map tab, which edited it, is gone (2026-10-28). The patch document
  (`patch.rs`, `PatchDoc`) and `MapObjectPath` were deleted in M3; projects
  were upgraded into format 6 by `lpa-upgrade`, which keeps its own frozen
  copy of the old readers.

## The mapping model

The words are names.html's (draft 3): **part, shape, segment, lamp,
address, span, gap, patching, patcher, cursor, start, flow, override,
direction, offset, problem**. A lamp is one addressable light; a channel is
one number a port sends (an RGB lamp takes three).

**Parts make a tree; its leaves are segments.** A fixture's parts are shapes
(path, polygon, circle, grid, filled polygon, points), groups and repeats.
`part::render` turns them into the rendered tree: a repeat's copies are
generated parts (`panel@2`), a circle's rings and a split path's subpaths
are generated segments (`circle1/r3`, `path1/s2`), and every other shape is
its own segment. A **segment** is one run of lamps on one port, the only
thing that holds lamps; one segment never has two addresses. The segments in
tree order are the fixture's lamp order, and their positions sit in one
exact-size buffer.

- Geometry is today's arithmetic, never a copy: each shape's lamps come
  from `resolve_into` on a one-object document, so a layout renders the
  lamps `resolve()` gives, bit for bit.
- A circle holds rings with keys (`r1`, `r2`…), listed outside in, drawn
  from the outside or the inside, one segment each or one for all
  (`split: "one"`: today's ring set, which `Circle::from_ring_set` turns
  into a circle with every lamp where it was). A ring of radius 0 is the
  centre and holds one lamp.
- Ids are flat and never reused: `[a-z][a-z0-9_-]*` for a part, minted from
  the kind and a counter (`path3`); a node adds its key and its repeat
  copies, innermost first, trailing zeros dropped (`x@0@3`).

**The patcher** (`patching::patch_mapping`) walks one fixture's tree with a
**cursor** (output, port, channel) and gives every segment its **address**:

1. Entering a node, each field takes one step: the node's override, else its
   part's start, else its parent's flow, else Keep. A step keeps, sets or
   adds; amounts are in lamps or channels, converted at the segment.
2. Flows (`end_to_end`, `one_per_port`) render starts for children; the
   patcher sees starts only.
3. A segment lays its run at the cursor and moves it on. One that doesn't
   fit what is left of its port wraps to the next port, unless it is the
   first after a port or channel Set: that one stays, is reported, and
   lands as far as the port reaches.
4. Past the last port, nothing lands until a port is Set again. Off lays
   nothing and moves nothing. Leaving a node never moves the cursor.
5. Overrides are by id and never retargeted. Nothing flows across fixtures.
6. What it can't do it reports (`Problem`), naming the node or port.

The result (`Patching`) holds every segment's address, direction and
offset, each port as a row of **spans** (segments and gaps), the problems,
and what each node did and why (`Walked`).

## Map2d format 6

One `*.map2d.json` per fixture holds both halves (`map2d6::{read, write,
peek_format}`). The four journeys' documents are in `tests/format6/`; the
circular display as soldered:

```json
{
  "format": 6,
  "view_box": [0.0, 0.0, 100.0, 100.0],
  "next": { "circle": 2 },
  "start": { "output": { "set": "main" }, "port": { "set": 0 },
             "channel": { "set": { "lamps": 0 } } },
  "parts": [ { "id": "circle1", "name": "display", "kind": { "circle": {
    "center": [50.0, 50.0], "start_angle_deg": -90.0, "dir": "cw",
    "drawn_from": "outside", "next_ring": 6, "rings": [
      { "key": "r1", "radius": 42.0, "count": 24 },
      { "key": "r2", "radius": 32.0, "count": 16 },
      { "key": "r3", "radius": 22.0, "count": 12 },
      { "key": "r4", "radius": 12.0, "count": 8 },
      { "key": "r5", "radius": 0.0, "count": 1 } ] } } } ],
  "overrides": { "circle1/r3": { "port": { "set": 1 },
    "channel": { "set": { "lamps": 0 } }, "direction": "reverse" } }
}
```

- `view_box` (required): the fixture's coordinate system, `[min_x, min_y,
  width, height]` in the layout's units, y down, as an SVG `viewBox`; the
  fit frames it (a box of no width or height frames the lamps' own
  bounds). A document without one is refused by name. Lamps have no size:
  a `sample_diameter` is refused by name (fixture-space theory, D7, D11,
  D28).
- Kinds: `path` (`d`: SVG path data, `M` and `L`; `H`/`V` read as lines;
  `Z`, curves and arcs refused by name until built; a move inside a path is
  the jumper; `split: "subpaths"`), `polygon`, `grid`, `filled_polygon`
  (today's fields), `circle`, `points` (`at`), `group` (`parts`), `repeat`
  (`center`, `count`, `part`).
- Starts are per-field ops, an absent field Keep: `{"set": "main"}`,
  `{"set": 0}`, `{"add": 1}`, `{"set": {"lamps": 0}}`,
  `{"add": {"channels": 13}}`. Outputs are named in the ops; there is no
  outputs list. A part's `start` and `flow` apply to every copy of it.
- Overrides are keyed by node id; their fields are ops (an explicit
  `"keep"` holds the cursor), `off`, `direction` (`"forward"`/`"reverse"`)
  and `offset` (lamps). One naming no node is kept.
- Counters: `next` (per kind word), `next_ring` (per circle). Format 6
  saves no lamp facts; the lamp-model plan adds them with its own bump.
- `read` takes format 6 only. A newer document is refused as newer, and
  1–5 as "the older layout document" (the upgrade is the host's,
  `lpa-upgrade`). Without `std` (the device), every error's text is one
  sentence, `map2d6::DEVICE_REFUSAL`: the device runs every check a host
  runs and drops only the words, so a board accepts exactly what a host
  accepts. Writing gives one canonical form, so a document read and written
  back is the same bytes.

## The `edit` feature

`edit/` holds the edits an editor makes to a `part::Mapping`, each one
function that keeps the document whole: add, insert, move, remove and
rename a part; wrap a part in a repeat, unrepeat it, expand a repeat into
its copies; set a repeat's count or a shape's lamps; add and remove
rings, set a ring's radius, a circle's way round and split, a path's
split, where a shape is drawn from, a lattice; set starts and flows; set
and clear override fields. Every editor gets the same answers: Studio's
Graph, the app agent through the Graph's offers, and someday `lp-cli`.

Some edits change which ids exist (a rename, a wrap in a repeat, an
expand, fewer copies). Overrides are keyed by id and are never
retargeted: an edit carries an override to its node's new id, or keeps
it as it was for the patcher to report as gone. An edit that could keep
one only by pointing it at a different node is refused, and a refused
edit changes nothing.

The feature is off by default. `lpa-graph` (Studio's Graph) turns it on.
No firmware does, and neither does `lpc-engine`, so the C6 image never
compiles it. Its statements are `tests/edit.rs`:

```sh
cargo test -p lpc-mapping --features edit
```

## Space

`space/` answers, for one fixture and one source: **given a lamp at a
fixture-space point, which coordinate does the source see, or is the lamp
dark?** The fixture's box is its `view_box` (`ViewBox`, y down). A source
has a box of its own (`ProductBox`): a view for a shader (`unit`: −1…1, y
up; `shadertoy`: pixels, y up, the fixture box's shape with N = 64 on its
short side, N × N with no fixture) or an extent for something bounded (a
fluid's grid). The fixture's `Fit` (contain, cover or fill; nine
alignments; scale; rotation, clockwise as drawn) gives one affine T per
source (`SpaceTransform`). The y flip is inside T. A lamp outside the box
is `Dark`. A sample past a bounded extent is `Outside` until the fixture's
`OutsideRule` (solid, clamp, wrap, mirror, per axis) resolves it. `unit`'s
uniforms come through T (`UnitScope`), a 1D source lands as the fixture
says (`Landing1d`), and along the order lamp `k` of `N` sits at `k/(N−1)`
(`order_position_q16`).

T is exact fixed point: built in `f64`, applied in integers to box-relative
Q2.30 coordinates, with Q16.16 out, rounded to nearest. A 30,000-lamp strip's
last lamp lands on exactly 1.0, and a 40 m box in millimetres does not
overflow. Cover of the unit view on the lamps' own bounds gives today's
pattern frame bit for bit. The statements are in `tests/space_statements.rs`:

```sh
cargo test -p lpc-mapping --test space_statements
```

Nothing in the engine calls it yet. The theory is planning
`lp2025/2026-10-28-0935-fixture-space-theory` (`theory.md`), and the ADR
"Fixture and shader space" will record it.

## Modules

| Module | What |
|---|---|
| `space/` | Fixture and shader space: the box, the fit, T, the outside rule, the 1D landing, `unit`'s scope (above) |
| `part/` | The parts, the rendered tree, ids, segments, `render`, `render_layout` (the same tree with no lamp placed: what a patch walks), `Tree::take_positions` (the engine's carrier takes the positions), `Mapping::same_layout` (a patching-only edit renders nothing) |
| `patching/` | Starts, flows, overrides, the cursor, `patch_mapping`, `patch_mapping_placed` (no walk record: the engine's), spans, problems |
| `map2d6/` | Format 6: its document structs, a small JSON reader (`json_text`, no serde on the read path), path data, `read`, `write`, `DEVICE_REFUSAL` |
| `edit/` (feature `edit`) | Edits that rewrite references (insert, move, rename, wrap in a repeat, expand, rings, starts, flows, overrides): Studio's Graph; never compiled into firmware |
| `sorted_set` | Sorted-`Vec` id sets the reader, render and patcher check ids with (smaller on the C6 than a B-tree) |
| `map2d_doc`, `map2d_resolve`, `map2d_fit`, `map2d_object_id`, `map2d_error` | The older document (formats 1–5) and its resolver; the resolver is also the new model's geometry |
| `editor_meta` | Studio's `editor.json` |
| `import/` | SVG import into the older document (the logo-sign generator uses it) |
| `corpus` | Shared test scenes in the older document (Studio's fixture-face stories) |

## The older map2d document (formats 1–5)

What follows describes `Map2dDoc`, which no project at format 12 holds: the
upgrade rewrote every one into format 6. It stays for the resolver (the new
model's geometry) and the readers named at the top.

A mapping document (`fixture.map2d.json`) is a format-versioned JSON asset,
opaque to the slot system, holding parametric objects whose order **is** the
wiring order:

```json
{
  "format": 1,
  "sample_diameter": 2.0,
  "objects": [
    { "name": "panel", "shape": { "grid": { "origin": [0, 0],
      "cols": 16, "rows": 16, "pitch": 26,
      "routing": "snake", "start_corner": "tl" } } },
    { "name": "button", "shape": { "ring": { "center": [600, 200],
      "radius": 90, "outer_count": 16, "rings": 2, "spacing": 45 } } },
    { "name": "stroke", "shape": { "path": { "count": 23,
      "points": [[51.2, 43.8], [33.6, 91.7], [78.4, 92.8]] } } }
  ]
}
```

A path may mark some of its segments **inert** — jumper wire that carries no
lamps — so one physical channel stays one object:

```json
{ "name": "channel 1", "shape": { "path": { "count": 24,
  "points": [[100, 300], [100, 100], [160, 100], [160, 300]],
  "gaps": [1] } } }
```

Segment `i` runs `points[i] → points[i+1]`. Lamps distribute evenly over the
**active** arc length only, so pitch stays uniform across the whole channel
(fixed-pitch strip cut at a hub and jumpered), and an inert segment emits no
lamp entries at all — placeholder lamps would shift every downstream wiring
index. `reversed` mirrors the gap indices with the points, so the same physical
segments stay inert whichever end the data enters from. Gaps are a format-2
construct: an older build would parse the field away and light the jumper.

A shape may also be **repeated**: `repeat` wraps an inner shape and turns
`count` copies of it evenly around a full circle about `center`, so one
authored sector becomes a whole dome:

```json
{ "name": "sector", "shape": { "repeat": {
  "shape": { "path": { "count": 12, "gaps": [1],
    "points": [[200, 140], [200, 60], [240, 60], [240, 140]] } },
  "center": [200, 200], "count": 5 } } }
```

Instance `k` is the inner shape turned `k * (360 / count)` degrees; instance 0
is the inner shape untouched. Instances are consecutive in wiring order and
each resolves to its **own span** — a repeated object is N physical strands,
not one long run, which is what the fixture's honest spans and the output
face's strip boundaries need to see. Repeats nest (spans multiply; the
innermost instances are the strands), and the editor clamps `count` to
`1..=64`. Like gaps, `repeat` is a format-2 construct — and the sharper case:
an old build cannot ignore an unknown shape variant without losing every lamp
the object carries.

A polygon outline can also be **filled** with a lamp lattice — the shaped
matrix (a custom PCB cut to a silhouette), where the count is not authored at
all:

```json
{ "name": "badge", "shape": { "filled_polygon": {
  "points": [[0, 0], [96, 0], [48, 83]],
  "pitch": 16, "angle_deg": 0, "origin": [0, 0],
  "routing": "snake", "start_corner": "tl" } } }
```

A cell is populated iff its center falls inside the outline, and no closer
than ε = `pitch × 1e-3` to any edge. That inset is the determinism tie-break —
a center exactly on an edge is always out, so the count never turns on which
way a float rounded — and the damper that stops the lattice blinking while a
vertex is dragged. `angle_deg` turns the *lattice*, not the outline (about the
outline's bbox center); `origin` slides its phase. Routing walks populated
cells row by row exactly as `grid` does, except that snake parity counts only
rows that actually hold cells: an empty row is not a row the chain reaches, so
it does not flip the next row's direction.

The count being derived is the load-bearing part. `filled_polygon_cells` is
the single walk both `resolve` and `shape_lamp_count` run, so the two cannot
disagree; an outline that admits no cell is a resolve-time error naming the
object, never a silently empty span. Stride is 1 (rows vary in length — no
honest period), and `align` is deliberately absent: stroke alignment says
nothing about a field of lamps. `filled_polygon` is a format-5 construct, the
unknown-variant case again.

Shapes are externally tagged (`"shape": {"grid": {...}}`) rather than using a
`kind` field: the repo bans serde `tag`/`untagged`/`flatten` in the firmware
dependency graph (Content-machinery flash cost — `scripts/check-serde-content.sh`).
`repeat`'s inner shape is boxed for the same reason a nested enum usually is:
to keep `Map2dShape` small.

`resolve(doc)` produces the ordered lamp list — doc-space positions in
wiring order; a lamp's index in that order is its one address here (the
resolver derives no wire-side addresses). `resolve_into` is
its streaming form: positions and spans into caller-owned buffers, one
exact-size 8 B/lamp allocation and no per-lamp rows — what the engine loads
through, since at dome scale the rows were the load's largest single ask.
`fit_points` maps doc space into a fixture render target without
stretching (`fit_points_in_place` does it on the caller's buffer); an
optional `canvas` field (e.g. an imported SVG viewBox) overrides the framed
region.
`import::svg_to_doc` converts the strict Illustrator-friendly SVG subset
(`path:N,count:N` groups) into a document — import is a one-time conversion,
not a runtime source of truth. CLI form:

```sh
cargo run -p lpc-mapping --example svg_to_map2d -- path/to/mapping.svg > fixture.map2d.json
```

## Boundary

Schema + pure geometry only: `no_std + alloc`, sans-IO, no engine types, no
UI, dependencies limited to `serde`/`serde_json`/`libm`. The device resolves
documents at project load, so everything on the resolve path must stay
no_std and deterministic (pure `f32` + `libm` — identical output on every
platform).

## Schema evolution

The first three rules hold for every map2d format, 6 included; format 6's
writer always stamps 6, since no older format can hold its document.

**Additive fields need no bump.** Unknown fields are ignored, so a new
optional field stays readable by older parsers. Dense geometry is plain JSON
arrays today; a packed base64 alternative (e.g. `points_packed` beside
`PathShape::points`) is deliberately left room for as an additive field —
mapping documents ride Studio's whole-body asset pipeline with a 10 KiB body
budget, and the corpus enforces headroom in tests.

**Constructs an old parser would misread bump `format`, and old parsers
hard-fail — by design.** An unknown shape variant cannot be ignored the way an
unknown field can: doing so would silently drop lamps from someone's fixture.
Neither can a field that changes what the *existing* fields mean — `gaps`
re-parameterizes the whole path, so a format-1 build that dropped it would
light the jumper wire and shift every downstream index. So a document using a
newer construct declares a higher `format`, and a build that reads up to a
lower one refuses the whole document. Loud refusal is the chosen posture: a
build meeting data it does not understand should stop, not guess.

**`format` is peeked before the document is parsed.** `Map2dDoc::from_json`
deserializes the `format` field alone first, so a newer document fails as
`UnsupportedFormat { found, supported }` — "this build reads up to N" — rather
than as serde's opaque "unknown variant" parse error. Hosts word that as *this
document needs a newer LightPlayer*.

**Writers stamp the minimal required format.** `Map2dDoc::required_format`
reports the lowest format able to represent a document's actual content, and
`normalize_format` stamps it; the editor session runs it on every commit. A
document therefore declares what it *needs*, not what wrote it — strip the last
newer construct out and it drops back to the older format, readable by older
firmware again.

**Editors refuse to edit what they cannot parse, and never rewrite it.**
Studio's Graph (`/graph`; the old `#/mapping` page and the Map tab are gone)
refuses a mapping it cannot read by name and writes nothing: the stored
document survives open → close byte-identical.

## Corpus

`corpus::{basic_button, cat_ears, panel_16x16, gapped_path, repeated_sector,
fyeah}` are the shared test scenes. The two format-2 archetypes are
`gapped_path` (one channel that jumpers across an inert segment) and
`repeated_sector` (a small-dome: one gapped sector repeated five times — one
object, 5 strands, 60 lamps); the last is the real fyeah sign, derived from
its mapping SVG via the importer: 219 lamps, 2 universes. Studio stories and
editor fixtures should reuse these rather than inventing new geometry.
