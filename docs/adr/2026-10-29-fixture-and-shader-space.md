# ADR: Fixture and shader space — one box, one fit, one transform

- **Status:** Accepted. Yona agreed the theory point by point on
  2026-10-28. On 2026-10-29, after looking at the look report and pictures
  of every shader under each answer, he answered its open questions.
- **Date:** 2026-10-29
- **Deciders:** Yona, Photomancer
- **Plan:** `lp2025/2026-10-28-0935-fixture-space-theory` (`theory.md`,
  `requirements.md`, `look-report.md`, `look-pictures/`)
- **Supersedes in part:** `2026-09-24-pattern-space.md` (its frame: the
  lamps' box becomes the fixture's box through a cover fit; `pattern`
  becomes `unit`); `2026-08-07-two-sided-space-model.md` (who decides how
  a 1D source lands moves to the fixture). Each carries a dated amendment.
- **Superseded by:** None
- **Related:** `2026-10-28-graph-is-the-project-canvas.md` (one camera, the
  fixture card), `2026-08-24-canvas-object-renderables-and-alignment.md`
  (lamps drawn from pitch), `2026-09-06-direct-sampling-bounded-batches.md`
  (texture-area sampling retires), `2026-10-27-mapping-model-parts-segments-patcher.md`
  (format 6's `view_box`), `2026-10-28-project-format-policy.md` (the
  format step), `docs/ux/anatomy/fixture-card.md` (the fixture card).

## Context

Before this decision there were five rectangles and two kinds of shader, and
nothing kept them in step:

- the map2d `canvas` (now `view_box`);
- the fixture's `render_size`;
- the lamps' own bounds, which framed pattern-mode shaders;
- the Graph's card, which took its shape from `render_size`;
- a picture placed by an inverse of the engine's fit.

Pixel shaders saw the canvas contain-fitted into `render_size`, y down.
Pattern shaders saw the lamps' bounds, long side −1…1, y up. The Graph's
Width/Height rows rewrote the canvas while the engine fitted that canvas into
an unchanged `render_size`, so a 256 × 1 strip became an 8 × 8 picture. Some
test strips had a square canvas contained in a long, thin `render_size`, so
their shaders saw the whole strip in a sliver of one render pixel. The
earlier pattern-space decision fixed only the shader's half, and made it
opt-in.

A shader is a function over a continuous, unbounded space, with uniform
metadata about where it is rendering. There is no one standard way for
shaders to meet the outside world. So we model the world once and name a
very small menu of ways in.

LightPlayer has no field users yet: only Sean's testing and Yona's own
boards. So the bar for upgrading existing projects is that the shaders still
look OK, not that every coordinate is kept.

## Decision

### 1. Coordinate systems

- **A fixture's coordinate system is an SVG root.** Its box is the map2d's
  required `view_box`, `[min_x, min_y, width, height]` in the drawing's own
  user units, y down. Its physical size is optional: a pair of numbers in
  millimetres in the map2d document. Without one, the size is nominal and
  the rulers read in user units.
- **A product's box** (a shader's, a fluid's, an image's) is a **view** or
  an **extent**. A view is unbounded and its box is a suggestion: a
  shader's default view is 1:1. An extent is bounded: nothing exists
  outside it. Any product may declare an extent, shaders included.
- 2D and 1D only. Domes are 2D surfaces, mapped by projection.

### 2. The fit and the one transform

- The fixture authors a **fit**: a mode, an alignment, a scale and a
  rotation. The modes are contain, cover and fill, with CSS `object-fit`
  and SVG `preserveAspectRatio` semantics. The alignments are SVG's nine,
  `xMin…xMax` × `yMin…yMax`, where `yMin` is the top. The default is cover,
  centred, scale 1, no rotation.
- Scale and rotation act about the placed product's centre, after the mode
  and alignment. Rotation is in degrees, clockwise as the fixture is drawn,
  CSS's `rotate()`. There is no free offset.
- From the fit, the fixture's box and the product's box comes **one affine
  T from fixture space to product space**. The y flip between a y-down
  fixture and a y-up interface lives inside T and is never seen.
- **Every lamp sample goes through T, and so does Studio's picture of the
  source under the lamps**, so what you see is what the lamps sample.
  Lamps stay in fixture space.
- **Each source derives its own T** against its own box, recomputed when
  the source changes. A playlist cross-fade samples each entry through its
  own T. T is six numbers, so a source change touches no lamp.

### 3. Two outsides

- **Outside the fixture:** a lamp outside its fixture's box is dark: on the
  LEDs, in the Graph, everywhere. The box edges are inside.
- **Outside the product:** a sample past a bounded extent follows the
  fixture's **outside rule**, one of the GPU address modes per axis: solid
  colour, clamp, wrap or mirror. The default is clamp. Products do not
  suggest an outside rule. A bounded shader is not run where the rule gives
  the solid colour.

### 4. How a shader sees the world: `unit` and `shadertoy`

- **`unit`** is LightPlayer's own interface, and the one the catalog and
  agents write. Its view is −1…1, centred, y up: the square around the unit
  circle. Cover of that view on a box is the old pattern mode: the box's
  long side runs −1…1.
  - `patternExtent`: the fixture's box as the shader sees it, the
    half-size of its bounds through T. Cover on a 3:1 box gives (1, 1/3).
  - `patternPitch`: the lamp pitch through T, scaled by √|det T|, which is
    the fit's one scale for cover and contain.
  - `lampCount`.
  - `"coords": "unit"` renames `"pattern"`, and an absent `coords` means
    `unit`.
- **`shadertoy`** is a porting door, not a peer. It gives coordinates
  only: no `mainImage`, `iTime` or `iMouse`.
  - `pos` is `fragCoord` in pixels, origin bottom-left, y up.
  - `outputSize` and `iResolution` take the **shape of the box of the
    fixture that samples it, with N = 64 on the short side**. A 3:1 box
    gives (192, 64) and a 1:3 box (64, 192). A render with no fixture
    behind it gives N × N. A shader that divides by `outputSize.y` keeps
    the proportions it was written for.
  - In 1D, `pos.x` is `k + 0.5` and `outputSize` is `(N lamps, 1)`.
  - Today's pixel shaders become `shadertoy`. The format step writes
    `"coords": "shadertoy"` on each one and adds one line,
    `pos.y = outputSize.y - pos.y;`, at the top of `render_2d`, because y
    flips. No other GLSL is rewritten.
- There is no `uv` (0…1) interface: it is one line from either.

### 5. How a 1D source lands: the fixture decides

- A fixture has two coordinate systems: its drawing (2D, the box) and its
  order (1D, position along the lamps). A 1D source reads one or the other,
  and the fixture decides which.
  - **Along the order**, the default: lamp `k` of `N` sits at `k/(N−1)`,
    exactly, as WLED and FastLED effects expect.
  - **Across the drawing**: through the fit's T, then a projection to a
    line (extrude x or y, radial, angular, with mirror and flip).
- The shader's `in_2d` and `in_1d` answers, the fixture's `force` flag and
  `strip_order_meaningful` retire, and so does the centre scanline.
- The projection code stays on the producer's side. The request carries the
  fixture's choice as the only authority.

### 6. Point sampling

- Fixtures point-sample: the shader runs once per lamp, through T.
  `texture_area` sampling retires, along with its default, `render_size` and
  `sample_diameter`. If area sampling is ever needed, it comes back as a few
  points per lamp.
- Textures stay for consumers that want a picture: previews, probes, the
  bus. A render with no lamps behind it renders the shader's default view:
  −1…1 for `unit`, N × N pixels for `shadertoy`. It no longer treats every
  texel centre as a lamp.

### 7. Precision

T is built in `f64` and applied as exact fixed point. The per-lamp input is
a box-relative fraction in Q2.30, and the output is Q16.16. Each output row
shares one shift, with the larger coefficient kept to 29 bits; products and
sums stay inside `i64`, and the result is rounded to nearest and saturated.
A 30,000-lamp strip's last lamp lands on exactly 1.0, and a 40 m box in
millimetres does not overflow.

This is why `unit` beats dividing `fragCoord` by `iResolution` inside the
shader: in Q16.16 on the device, that division quantizes the scale to
1/65536, which on a long strip is a visible zoom error.

### 8. Importing an SVG

The importer is not built yet; these are its rules.

- A file without a `viewBox` gets `0 0 width height` from its `width` and
  `height`, else from the drawing's bounds.
- Physical size follows SVG's own unit rules (1 in = 96 px, 1 mm ≈ 3.78 px)
  and is stored in millimetres. A bare-pixel file has only a nominal size.

### 9. How upgraded projects keep their look

The format step writes each fixture's box and fit by one rule. The bar is
that the shaders still look OK.

- **Fed by `unit` shaders:** the box is the lamps' bounds, and the fit is
  cover, centred. An axis thinner than one pitch is padded symmetrically to
  one pitch, never past the long side. The far edge is rounded up, so the
  box holds its own last lamp.
- **Fed only by pixel shaders:** the box is the region of the drawing that
  the old `render_size` covered: the old canvas grown to the render's shape,
  centred. The fit is fill. That keeps `pos / outputSize`. With `shadertoy`
  taking the box's shape, it keeps `outputSize`'s aspect too.
- **Fed only by pixel shaders, and squeezed today:** the lamps span less
  than a quarter of the render on both axes because the old canvas is not
  the render's shape. Here the box is the lamps' bounds (padded as above)
  with fill. The strip is un-squeezed on purpose.
- **A fixture fed by both:** `unit` wins. The repo has none.
- The boxes the format-12 upgrade engineered (`[0, 0, 1, h]`) stay as
  numbers wherever this rule keeps them.

Measured on every project in the repo (76 projects, 131 fixtures that a
shader samples per lamp), against a frozen copy of today's arithmetic that
matches the live engine bit for bit:

- No lamp goes dark.
- Every `unit` lamp moves at most 2/65536.
- Every pixel lamp of a fixture that is not squeezed moves at most two of
  today's own steps (1/65536 of the render). That is at most 1.3% of a
  lamp pitch, on the two densest domes.
- The 27 squeezed test and catalog strips are un-squeezed: their chases and
  meteors now run the length of the strip.

Pictures of each fixture's own shader under each answer are in the plan's
`look-pictures/`.

### 10. The Graph

- A fixture is drawn in its own coordinate system, with rulers in its
  units: millimetres or metres when it declares a size, user units when it
  does not.
- Its card is a standard width, 240 px at zoom 1. Its height follows the box
  ratio, capped at 3× the width, with a minimum set by the chrome. A
  256 × 1 strip is a line. There is one camera, and no resize gesture.
- The box is set in the card's Box section, which shows the box, the
  physical size and the parts' bounds. It offers "Fit the box to the parts"
  (the bounds plus half a pitch) and warns about lamps outside. A new
  fixture's box is 500 × 500 mm.
- The fit, the outside rule, how a 1D source lands and the shader's
  interface are picked from selectors that say what each option does.
- The picture under the lamps is the source as this fixture sees it,
  sampled through the same T, faded past the box. The card is drawn in
  `docs/ux/anatomy/fixture-card.md`.

## Consequences

- One project-format step rewrites every fixture and shader: the box and
  fit by section 9's rule, `coords` on every shader, the y flip in pixel
  shaders. It also removes `render_size`, `sampling`, `transform`,
  `consume` and `strip_order_meaningful`. The step ships with its tests and
  migrates the repo's projects in the same change. The wire version bumps
  once for it.
- The step and its sample-parity test compare against a frozen copy of
  today's arithmetic (`lpa_upgrade::v12`), never against the live engine,
  whose meaning changes in the same change.
- The engine applies T per lamp in fixed point. The size, heap and
  first-light cost on the ESP32-C6 is measured when it lands, and this
  record gets the numbers.
- A pixel shader that uses a length in absolute pixels (a radius in render
  pixels) sees it scale by 64 over the old render's short side.
  `projects/test/basic2` and the two `perf` shaders use `outputSize * 0.5`
  as a centre, which stays the centre.
- A shader written for the Shadertoy corpus needs its `fragCoord` read as
  `pos`, and nothing else, as long as it only uses coordinates.
- Studio offers every choice above as typed offers in core, so the app's
  agent presses them like a person.

## Alternatives Considered

- **The lamps' box as the frame** (the pattern-space decision). It hides
  the box, so the Graph could not show what the shader saw. It only covers
  shaders that opt in.
- **A frame per shader.** Two shaders on one fixture would disagree about
  where the fixture is. The fit belongs to the fixture.
- **Keeping texture-area sampling.** It was the old way, from before direct
  sampling. Every fixture in the repo but one already samples directly.
- **Long side versus short side as an interface.** That is a fit (cover
  versus contain), not a different interface.
- **A `uv` interface.** It is one line from either interface.
- **The producer deciding how a 1D source lands** (the two-sided model's
  "the producer knows its own opinion"). The same effect has to land
  differently on a scarf and on a matrix, and only the fixture knows which
  it is.
- **A square N × N `shadertoy` view everywhere.** It changes the PLAYFUL
  choker's own shader (it reads `pos / outputSize.y`) under every
  upgrade answer.
- **Upgrade answers that were measured and not chosen:**
  - The old canvas with fill for pixel fixtures. It moves the eaves by 22
    lamp pitches and the F-YEAH sign by 46, and switches off 32 of
    five-wire's 80 lamps.
  - The old canvas everywhere. It also shifts the `unit` patterns by up to
    3 lamp pitches.
  - The render's footprint for every pixel fixture. It keeps the squeezed
    strips squeezed, with boxes like `[-5950 0 12000 100]`.

## Follow-ups

- "Open the box": an editor given over to one fixture, where the box is
  the frame.
- An "actual size" fit (1 source unit = N mm), which would give fixtures a
  shared physical frame.
- A resize gesture for the box, and a free offset in the fit.
- Products suggesting an outside rule (a simulation that wraps around a
  cylinder).
- Area sampling as a few points per lamp, and a physical lamp size.
- 1D position by physical length along the parts.
- Volumetric 3D.
- The device's flash, heap and first-light cost, filled in here when the
  engine samples through T.
