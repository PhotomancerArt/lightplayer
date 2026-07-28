# lpc-mapping

The 2D LED fixture mapping domain: the authored mapping **document** schema
and the single deterministic **resolver** shared by the engine, the device,
and Studio.

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

Shapes are externally tagged (`"shape": {"grid": {...}}`) rather than using a
`kind` field: the repo bans serde `tag`/`untagged`/`flatten` in the firmware
dependency graph (Content-machinery flash cost — `scripts/check-serde-content.sh`).

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

## Boundary

Schema + pure geometry only: `no_std + alloc`, sans-IO, no engine types, no
UI, dependencies limited to `serde`/`serde_json`/`libm`. The device resolves
documents at project load, so everything on the resolve path must stay
no_std and deterministic (pure `f32` + `libm` — identical output on every
platform).

## Schema evolution

`format` gates parsing (documents newer than `MAP2D_FORMAT` are rejected;
unknown fields are ignored, so additive changes need no bump). Dense geometry
is plain JSON arrays today; a packed base64 alternative (e.g. `points_packed`
beside `PathShape::points`) is deliberately left room for as an additive
field — mapping documents ride Studio's whole-body asset pipeline with a
10 KiB body budget, and the corpus enforces headroom in tests.

## Corpus

`corpus::{basic_button, cat_ears, panel_16x16, fyeah}` are the shared test
scenes (the fourth is the real fyeah sign, derived from its mapping SVG via
the importer: 219 lamps, 2 universes). Studio stories and editor fixtures
should reuse these rather than inventing new geometry.
