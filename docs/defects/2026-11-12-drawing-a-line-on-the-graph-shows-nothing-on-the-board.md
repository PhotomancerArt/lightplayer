---
status: fixed
found: 2026-11-12      # how: Yona, mapping his LED skirt in Studio's Graph on a C6
fixed: PENDING         # the Graph's board highlight PR
area: lpa-studio-core — the Graph's board highlight while drawing; lpc-engine — the output's `highlight` chase
class: stand-in-divergence
related:
  - lp-app/lpa-studio-core/src/app/graph/graph_handle.rs (input: the draft note)
  - lp-app/lpa-studio-core/src/app/graph/graph_highlight.rs (board_draft)
  - lp-app/lpa-studio-core/src/app/graph/graph_controller.rs (highlight, DRAFT_GAP_BLUETOOTH)
  - lp-app/lpa-graph/src/domain/fixture_overrides.rs (Drawing)
  - lp-core/lpc-engine/src/nodes/output/output_node.rs (chase_reach, ChannelOrders::order_for)
  - lp-app/lpa-studio-core/src/app/studio/studio_graph_journeys_tests.rs (drawing_a_line_lights_the_board_as_the_pointer_moves)
---
# Drawing a line on the Graph shows nothing on the board

**Symptom** — Mapping LEDs by drawing: you click out a line on the Graph
and move the pointer, expecting the LEDs the line will take to light so you
can stop where the strand really is. The canvas showed the draft (full
white, everything else off); the board showed nothing of it.

**Root cause** — Three gaps, each enough on its own:

1. Nothing woke Studio's actor while drawing. The page applies pointer
   input to the editor directly and notes the actor only when the project
   or the selection changed; a draft moving under the pointer changes
   neither.
2. Even woken, the board's highlight sent only the selection; the draft is
   not part of it, and isn't in the board's fixtures (nothing saves while
   drawing).
3. The board couldn't show lamps nothing is mapped to. An output's buffer
   ends at its fixtures' last lamp, so a chase named past it was clipped;
   and a chase lamp no fixture's run covers had no channel order, so the
   whole chase fell back to the white breath.

The canvas's preview was a stand-in the board never matched.

**Fix** — The page notes the actor when the draft changes
(`GraphNote::drafted`). While drawing, Studio sends the draft as an ordered
`chase:` on each output's `highlight` slot, in wire space, on the pixels it
will land on (the patcher's runs for the draft, clipped to the pixels the
port has now), and dims every fixture; over Bluetooth at most every 150 ms,
with the refresh tick sending the last of it. In the engine (firmware), a
chase may reach up to 1024 lamps past the rendered extent: the buffer grows
to its last lamp, dark but for the chase, and one more frame stays dark
there after it stops so the LEDs it lit go out. A chase lamp no run covers
takes the nearest run's channel order (a `Raw` run still falls back to the
breath, A2). The slot's text grammar is unchanged, so the wire didn't move.
The canvas draws the draft in the same pattern now, other lamps dimmed.
`drawing_a_line_lights_the_board_as_the_pointer_moves` cuts the walk
project's strip to 8 lamps of D0's 64, draws a line after it, and reads the
blue head at pixel 8 and the red tail moving on with the pointer from the
board's own frame; it failed on main.
