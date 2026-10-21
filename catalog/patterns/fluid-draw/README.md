# Fluid (`fluid-draw`)

A fluid you draw in. Drag on a connected board's picture in Studio and dye
swirls after your finger; let go and the fluid drifts on by itself. Leave it
for two seconds and three slow emitters (the "friends") come back to stir it.

## How it works

- `effect/draw` is a compute shader. It reads the `touches` channel — the
  fingers on the picture, by pointer id, in pattern space — and writes the
  Fluid node's emitters. Reading `touches` is what makes the picture
  drawable: Studio offers drawing only on a pattern that reads it.
- `effect/friends` is a second compute shader: the three idle emitters.
  It reads `touches` too, only to know when someone is drawing. The two are
  separate because a board compiles each shader within a fixed memory
  budget, and one shader doing both did not fit
  (`lpc-engine/tests/example_shader_compile_peak_memory.rs`). Both write
  `fluid.emitters`; the Fluid node merges them by id (friends 1–3, strokes
  16 and up).
- Each finger stamps four emitters along the chord from where it was last
  frame to where it is now, so a fast stroke stays a line. Velocity and
  direction come from that chord. A new finger dabs once.
- A stroke's colour comes from where it starts (lp2014's
  `(y·32 + x) mod 1` hue) and stays with that finger until it lifts.
- `effect/fluid` is the native Fluid node in pattern space
  (`"coords": "pattern"`): the grid spans the lamps' box, so the pattern
  works on any lamp shape.
- Knobs: **Fade** and **Viscosity**. They live on the draw shader, where an
  unwritten channel falls back to its default, and are handed to the Fluid
  node. Brush size and the idle time are constants (`BRUSH` in
  `draw.glsl`, `IDLE_AFTER` in `friends.glsl`): every declared input costs the shader's
  definition about a kilobyte, and a board sends a definition in one 16 KiB
  frame (`docs/defects/2026-10-20-a-node-def-over-a-read-frame-cannot-be-read.md`).

## Notes

- **Step rate.** The solver steps at `step_hz` (25), but on an ESP32-C6 the
  fluid renders at about 40 fps, so it steps about 20 times a second (half
  the frames repeat). Raising `step_hz` above ~20 buys nothing there
  (emulated, `lp-emu:esp32c6:t2`; see the drawing report in `docs/reports/`).
- **Thin and 1D lamp boxes.** Drawing is 2D in pattern space. On a long,
  thin box (an eave line) the 16 × 16 grid is stretched across it, so few
  rows land on the lamps; the friends wander within about ±0.3 of the
  centre line. A 1D strip reads the grid's centre line.
- **Colour** does not come from the palette yet: a compute shader cannot
  read one today.

Bench: emulated ESP32-C6, 2026-10-20 (see the report). Not yet on a desk
board.
