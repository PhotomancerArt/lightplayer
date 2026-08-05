# Zook dome conversion

One-off tooling that turns Bob Zook's Illustrator wiring sketch
(`mapping-attempt.svg`, committed here for provenance) into
`examples/zook-dome/fixture.map2d.json`. This is **not** the product SVG
importer — the sketch uses plain `<line>` elements, dashed jumper styling,
and filled arrowheads, none of which the importer's `path:N,count:N`
contract covers.

```bash
python3 scripts/zook-dome/convert.py --repeat   # the shipped form
python3 scripts/zook-dome/convert.py            # per-channel form, for comparison
```

Either mode regenerates the mapping document plus `validation.svg`
(resolved lamp dots over the strut structure, colored per physical
channel — compare it side-by-side with the sketch). Python 3 stdlib
only; deterministic. The committed `fixture.map2d.json` and
`validation.svg` are the `--repeat` outputs.

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
units — the per-channel lengths agree within ±1 unit.

## The shipped repeat form

The committed document is **one gapped sector × repeat 5** (map2d
format 2): channel 1's whole trail as a single 13-point path with
`gaps: [6]` (the jumper), wrapped in
`repeat { center: apex, count: 5 }`. `--repeat`'s fidelity check proves
this against the per-channel form:

- The five hand-drawn sectors are true 72° rotations of sector 1 to
  within **≤ 9 units** of drawing slop (vertex-by-vertex, after rotating
  each back); the least-squares rotation center lands 3 units from the
  authored apex.
- Per-lamp positions agree to single-digit mean deviation. The only
  outliers (one lamp each on ch2/ch3) are a knife edge in the OLD form's
  largest-remainder split (raw share ~171.5 flips a boundary lamp across
  the ~270-unit jumper). The gapped form's uniform pitch over active
  length is the physically faithful model — fixed-pitch strip, cut at
  the hubs and jumpered.

**Wiring order**: the sketch's sectors run counterclockwise in channel
order, while a map2d repeat turns clockwise, so repeat instance k is
physical channel `(1, 5, 4, 3, 2)[k]`. `output.json` lists its pins in
instance order — IO18, IO13, IO2, IO14, IO16 — so every physical sector
keeps the pin the per-channel form assigned it (ch1..ch5 =
IO18/IO16/IO14/IO2/IO13).

## Output topology note

`examples/zook-dome/output.json` authors the true physical split: five
wires of 300, listed in repeat-instance order (see above), the last as
the count-less remainder. Since the pooled-slot RMT work (PR #350), the
classic drives all five wires over its four RMT slots by re-binding pins
per transmission — the whole dome is live on one board.

```bash
lp-cli upload examples/zook-dome serial:auto
```
