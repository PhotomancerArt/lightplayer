# lpc-mapping

The LED fixture mapping domain: the authored mapping **document** schema, the
per-fixture **patch** document, and the single deterministic **resolver**
shared by the engine, the device, and Studio.

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

Shapes are externally tagged (`"shape": {"grid": {...}}`) rather than using a
`kind` field: the repo bans serde `tag`/`untagged`/`flatten` in the firmware
dependency graph (Content-machinery flash cost — `scripts/check-serde-content.sh`).
`repeat`'s inner shape is boxed for the same reason a nested enum usually is:
to keep `Map2dShape` small.

`resolve(doc)` produces the ordered lamp list — doc-space positions plus
derived DMX-style addresses (auto-flow at 170 RGB lamps/universe; wiring
order is primary, addresses are always derived from it). `fit_points` maps
doc space into a fixture render target without stretching; an optional
`canvas` field (e.g. an imported SVG viewBox) overrides the framed region.
`import::svg_to_doc` converts the strict Illustrator-friendly SVG subset
(`path:N,count:N` groups) into a document — import is a one-time conversion,
not a runtime source of truth. CLI form:

```sh
cargo run -p lpc-mapping --example svg_to_map2d -- path/to/mapping.svg > fixture.map2d.json
```

## The patch document

Mapping says where a lamp *is*; **patching** says which physical jack the
strand ended up in. A fixture is mapped once and re-patched on every install,
so the patch is its own document (`fixture.patch.json`) beside the mapping —
clearing a patch never disturbs a lamp position.

```json
{
  "format": 1,
  "entries": [
    { "range": { "start": 0,  "count": 22 }, "at": { "channel": 0 } },
    { "range": { "start": 22, "count": 22 }, "at": { "channel": 34 },
      "reversed": true }
  ]
}
```

`range` is fixture-relative — lamp indices in the wiring order above — and
`at.channel` is the lamp offset on the output's wire. Omitting `count` means
"to the end of the fixture", so a patched tail survives the fixture growing.
`reversed` lays a run down end-first (the strand fed from its far end); it is
a placement bit, unrelated to a fixture's own `wire_reversed` sampling.

Patches are **sparse**: `resolve_patch` anchors the entries and reflows every
unclaimed lamp after the highest anchored end, in fixture order. So no patch,
an empty patch, and a patch that anchors nothing all resolve to what auto-flow
would have produced — clearing restores the default rather than going dark, and
partial patching is the ordinary case. Two entries of one fixture claiming the
same lamp, or the same wire channel, are a document error (two *different*
fixtures colliding on one output is a runtime condition the engine degrades and
reports). `at.output` and a rotation `offset` are reserved in the schema and
refused by this build rather than silently ignored.

## Boundary

Schema + pure geometry only: `no_std + alloc`, sans-IO, no engine types, no
UI, dependencies limited to `serde`/`serde_json`/`libm`. The device resolves
documents at project load, so everything on the resolve path must stay
no_std and deterministic (pure `f32` + `libm` — identical output on every
platform).

## Schema evolution

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

**Editors refuse to edit what they cannot parse, and never rewrite it.** Both
Studio hosts (the fixture face and the standalone `#/mapping` page) render the
refusal in place of the editor: nothing mounts, no body is emitted, no autosave
is overwritten, and the stored document survives open → close byte-identical.

## Corpus

`corpus::{basic_button, cat_ears, panel_16x16, gapped_path, repeated_sector,
fyeah}` are the shared test scenes. The two format-2 archetypes are
`gapped_path` (one channel that jumpers across an inert segment) and
`repeated_sector` (a mini-dome: one gapped sector repeated five times — one
object, 5 strands, 60 lamps); the last is the real fyeah sign, derived from
its mapping SVG via the importer: 219 lamps, 2 universes. Studio stories and
editor fixtures should reuse these rather than inventing new geometry.
