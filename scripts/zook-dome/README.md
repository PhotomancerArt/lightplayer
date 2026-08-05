# Zook dome conversion

One-off tooling that turns Bob Zook's Illustrator wiring sketch
(`mapping-attempt.svg`, committed here for provenance) into
`examples/zook-dome/fixture.map2d.json`. This is **not** the product SVG
importer — the sketch uses plain `<line>` elements, dashed jumper styling,
and filled arrowheads, none of which the importer's `path:N,count:N`
contract covers.

```bash
python3 scripts/zook-dome/convert.py
```

Regenerates the mapping document plus `validation.svg` (resolved lamp
dots over the strut structure, colored per channel — compare it
side-by-side with the sketch), and prints a per-channel table. Python 3
stdlib only; deterministic.

## The dome

A 16' geodesic dome for Burning Man 2026, ~1500 LEDs on the struts as
**5 channels × 300 lamps** on one classic ESP32 (DOM-Z-102). The sketch
is a top-down projection; the controller sits at the apex hub and all
five channels start there, each snaking through one 72° sector to the
base ring. Solid colored lines are LED runs, dashed lines are jumper
wire (no LEDs), arrowheads give the data direction.

## How reconstruction works

1. Cluster hub circle centers + strut endpoints into canonical hubs
   (the drawing is hand-placed; one hub's copies span ~13 units).
2. Each `<g>` run group snaps its extreme endpoints to hubs; the
   arrowhead's leading `M` point names the far hub → a directed edge.
3. Per channel, find the **Eulerian trail** from the apex that uses
   every edge exactly once (greedy chaining is ambiguous at hubs the
   channel visits twice — each jumper creates an out-and-back).
4. Consecutive solid edges merge into one polyline ("stretch"); a
   jumper ends the stretch. Each channel gets exactly 300 lamps split
   across its stretches proportionally to arc length (largest
   remainder) — fixed-pitch strip, cut at the hubs.

Every channel resolves to 12 runs, 1 jumper, 2 stretches of ~2454 total
units — the per-channel lengths agree within ±1 unit, which is strong
evidence the five sectors are exact 72° rotations of one another.

## Output topology note

`examples/zook-dome/output.json` authors the true physical split: five
wires of 300 (IO18/IO16/IO14/IO2 + IO13 as the count-less remainder).
The classic drives four concurrent RMT channels today, so the fifth wire
parks (per-wire; the siblings stay live) until pin-mux channel pooling
lands. The Studio preview renders all 1500 lamps regardless.

```bash
lp-cli upload examples/zook-dome serial:auto
```
