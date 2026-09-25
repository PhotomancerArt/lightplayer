# ADR: Pattern space — a shader can see the lamps, not a texture

- **Status:** Accepted 2026-09-24 — accepted by Yona at the pattern review
  (reviewed by Yona at G-morning, pattern-library plan)
- **Date:** 2026-09-24
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

Before this change, a shader's `pos` came out of a four-stage chain of frames:
doc units → `canvas` → `render_size` → pixels. Every shader undid that chain
its own way. Most divided `pos` by `outputSize`. The PLAYFUL choker divided by
height, and plasma did something else again. `render_size` did two jobs: in
direct sampling it was only a unit, and in texture-area sampling it was a real
resolution. Its aspect ratio was matched to the board by hand. Nothing in the
chain knew how far apart the LEDs were, so "scale 1" meant something different
on every piece. A pattern library cannot be built on that: a pattern has to
work on any shape without editing (vision, pattern-library plan).

## Decision

**The rule, in one sentence:** pattern space is the lamps' bounding box,
centred at the origin and scaled uniformly so that its long side runs −1…1,
with y up; in 1D, `pos` runs 0 → 1 from the strand's first lamp to its last.

- **Opt-in, per shader.** Add `"coords": "pattern"` to the shader def. The key
  is an additive, optional field. When it is absent, or set to `"pixels"`, the
  shader gets today's pixel `pos` and `outputSize`, byte-identical on every
  backend. Old files round-trip unchanged, so there is no format bump.
- **Scope geometry.** Alongside `pos`, an opted-in shader can declare three
  render-request intrinsics, the same way it declares `outputSize`:
  - `uniform vec2 patternExtent;`: half the size of the lamp box, in pattern
    units. The long axis is exactly 1. A straight strip laid out in 2D gets
    `(1, 0)`.
  - `uniform float patternPitch;`: the mean distance between consecutive lamps
    within a strand, in pattern units. It is computed in one O(n) pass with
    O(1) memory, once per mapping load (pre-ruling DD1).
  - `uniform float lampCount;`

  `outputSize` still means the size of the render request.
- **Where it happens.** The transform runs on the host, as an exact integer
  affine on the Q16.16 sample coordinates that every backend consumes (rv32 and
  Xtensa `lpvm-native`, `lpvm-wasm`, the GPU tier). All backends therefore see
  the same coordinates. Nothing is compiled into the program, so switching
  `coords` costs no recompile.

  The transform is not a `pos * scale + offset` inside the shader because in
  Q16.16 that would round `scale` to 1/65536. On a 30,000-lamp strip, 1/(N−1)
  is two ulps, and the last lamp would land near 0.92 instead of 1.
- **Texture-area sampling** places each texel centre through the same
  lamp-box fit. A texel outside the lamp box gets a coordinate outside ±1,
  which is fine.

### Why the long side

A straight strip laid out in 2D has a lamp box of zero height. A rule based on
the short side would divide by zero there. A rule based on the long side gives
`extent.y = 0` and no NaN.

### Why lamp bounds and not the canvas

"Edge to edge" should mean across the LEDs, not across the empty margin of the
board. The canvas frames the mapping editor and the previews. It never frames
the shader's coordinates. On the choker, the canvas-based figure would have put
the y extent at ±0.27. The lamp box puts it at ±0.1377, which is what the
choker test asserts.

### Scope geometry versus sample geometry

Scope geometry describes the whole surface being rendered: extent, pitch and
lamp count. It is decided here.

Sample geometry describes the current lamp: which object it belongs to, its
distance along the path, and its wire index. It is out of scope. It can be added
later without breaking anything, because direct sampling already streams
per-lamp coordinates.

### What a request without lamps sees

A Studio canvas preview, the browser canvas, or a render-product probe has no
lamps behind it. For these, every texel centre counts as a lamp. The corner
texel centres map to ±1, and the pitch is one texel.

## Consequences

- A pattern written to the rule works on any mapping without editing, and it
  can convert between piece-relative and LED-relative sizes with
  `patternExtent / patternPitch`.
- Existing shaders are untouched. The CI goldens and the opt-out test pin that.
- A pattern-space texture render goes through the sampling path in bounded
  windows, not the synthesized texture loop. It is somewhat slower per texel,
  and it keeps an 8 B/texel persistent byte buffer for the texture write. On the
  wasm GPU tier it inherits the point pass's one-frame readback latency.
- The fixture keeps 24 B of scope geometry per mapping version.
- The ESP32-C6 image grows by 8,992 B against main at 1cd1f7d4e
  (2,448,880 → 2,457,872 B; headroom 687,856 B). Against the earlier mains it
  was measured on, the growth was 9,040 B (8fe93d9db) and 8,864 B.
- The wire proto bumps (24 → 25; lean-wire's follow-ups took 23 and the
  gradient-cycle pin took 24): the on-disk
  key is additive, but an old peer refuses a shader def carrying a field it
  does not know.

## Alternatives Considered

- **Compile the transform into the shader** (rename the author's `render_2d`
  and wrap it). This would cover every path inside the GLSL, but Q16.16 scale
  quantization makes it wrong at dome scale, and it means editing the author's
  source.
- **Change the synthesized texture loop's ABI** to take an origin and a step.
  That touches every backend's invocation, and a stepped Q16 accumulator has the
  same precision problem.
- **Pitch as the median nearest-neighbour distance.** This needs O(n²) time or
  an O(n) sort on the device. Both are rejected by DD1 for the ESP32-C6 at dome
  scale.
- **Normalize by the canvas.** Rejected above: the margin is not the piece.

## Follow-ups

- **Parked: a shared frame across fixtures** (vision D10, a rig-level scope
  geometry). Today each request lends its own fixture's box. A shared frame
  would be a second `ScopeGeometry` source, and nothing here prevents it.
- Deriving `render_size` from pitch was deferred by DD3.
- Sample geometry (per-letter, along-stroke, wire-order patterns).
