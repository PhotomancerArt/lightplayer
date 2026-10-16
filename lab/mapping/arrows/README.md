# lab-arrows

Arrows for the mapping lab: **a semantic arrow in, renderer-agnostic
geometry out.** Pure Rust, no IO, no dependencies at all.

The lab's canvas draws its wiring as curved arrows with words on them, and
the words were the messy part: a label floating level at the midpoint
wherever the curve happened to be, with a channel name on its way (D53).
This crate is the arrow piece of the lab's view model
(`lp2025/2026-10-10-1853-mapping-design-lab/view-model.md`): the model says
*what* to draw, with roles; the renderer only draws.

```bash
cargo test -p lab-arrows                                   # the statements
cargo run -p lab-arrows --example gallery > /tmp/arrows.svg
qlmanage -t -s 1600 -o /tmp /tmp/arrows.svg                # a PNG of it
```

## Semantic → geometry

- **`Arrow`** says what the arrow is: `from`, `to`; how it leaves and
  arrives (`EndDirection`: straight toward the other end, from a `Side` of
  the point, or an explicit direction of travel); its `Route` (straight, or
  a smooth cubic with a `Bend`; elbows later); a `Head` at the end and
  optionally at the start (triangle or chevron, with a size); and its
  `Label`s, each with text, a `LabelRole` (`Primary`, `Quiet`) and a
  preferred `Placement` (centre, near the start, near the end, or a 0–1
  fraction).
- **`Arrow::layout`**, or **`layout_arrows`** for a set (so their labels
  keep out of each other's way), gives an **`ArrowGeometry`**: the line as
  `Path` segments (move/line/cubic, and `to_svg()` for a `d`), cut back so
  it stops at a triangle's base; each head as a polygon (`closed`: fill it,
  else stroke it); and per label a `LabelFit` — `Along { moved }`,
  `Beside`, or `Dropped(reason)` — with, when placed, a `PlacedLabel`: the
  little box (`OrientedBox`: centre, width, height, angle), where to anchor
  the text, its rotation, anchor and baseline.

Everything comes back in the units it went in. Text is measured through a
`TextMeasure` the caller supplies (any `Fn(&str, f64) -> f64` will do);
`AverageCharWidth` is a stand-in, exact for a monospace font.

## How a label finds its spot

1. Its box is the measured text plus padding.
2. It wants a stretch of the line as long as the box that is nearly
   straight: the line strays from the stretch's chord by no more than
   three quarters of the box's half-height (`straightness`), so it stays
   behind the text from end to end. The stretch keeps `clearance` (plus a head's length) from
   either end.
3. Spots are tried from the preferred one outward, a quarter box-height at
   a time. The box sits centred on the line, turned to the chord's angle,
   flipped if need be so the text never reads upside down (angle in
   (−90°, 90°]).
4. If no stretch works, a level box beside the arrow's middle (above it;
   right of it, for a steep arrow; then the other side).
5. Across a set, labels are placed greedily — every primary before any
   quiet, then in input order — each taking its first spot that touches no
   placed box and no other arrow's head, preferring one no other arrow's
   line runs behind. A box beside an arrow is never put over another
   arrow's line, since it would read as that line's label. No free spot:
   `Dropped(Crowded)`.

Same input, same geometry: nothing reads a clock, a hash map or a random
number.

Known limit: the pass is greedy. A primary label that takes the middle of
a short arrow can leave too little room for a quiet one near the end, which
then drops as `Crowded` rather than nudging the primary aside.

## How the page will use it

The model builds one `Arrow` per edge (`edges.rs`: lamp → pixel, leaving
`Direct`, arriving from `Side::Left`; bindings leave from the right) with
the span label as `Primary` and the channel's name as `Quiet`, near the
end. It lays them out together, in screen units, once per frame, and the
page draws what comes back: the path, the heads, and each placed label as a
background-filled box with its text, rotated. Hovering tests
`Path::distance_to`. Integration is a later step; this crate depends on
nothing in the lab, and nothing in the lab depends on it yet.
