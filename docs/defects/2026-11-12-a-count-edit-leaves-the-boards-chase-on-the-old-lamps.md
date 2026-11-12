---
status: fixed
found: 2026-11-12      # how: Yona, mapping his LED skirt in Studio's Graph on a C6
fixed: PENDING         # the Graph's board highlight PR
area: lpa-studio-core — the Graph's selection on the board (`GraphController::highlight`)
class: state-conflation
related:
  - lp-app/lpa-studio-core/src/app/graph/graph_controller.rs (highlight)
  - lp-app/lpa-studio-core/src/app/graph/graph_highlight.rs (board_selection)
  - lp-app/lpa-studio-core/src/app/studio/studio_graph_e2e_tests.rs (a_selected_segments_chase_follows_its_count)
---
# A count edit leaves the board's chase on the old lamps

**Symptom** — With a line selected on the Graph, its LEDs on the board
show the selection's pattern: blue lead-in, red lead-out, a white band
sweeping between. Changing the line's lamp count (13 to 16, say) changes
the canvas, but on the board the pattern stays where it was: it ends at
the old last lamp, and the new lamps sit dimmed after it, so the chase
looks cut off.

**Root cause** — `GraphController::highlight` sent the board its fixtures'
`selection` texts (`chase:0-12`) only when the selection changed, and
returned early on `self.pulsed == selection`. A count edit keeps the same
target selected, so the recomputed text (`chase:0-15`) was never sent. The
resend was keyed on the selection, while what the board needs to hear
about is what it is told to show; the two moved apart on any edit that
moves the selected lamps (a count, a start, an insert before it).

**Fix** — The resend is keyed on what would be sent: the fixtures' texts
and the outputs' pulse, each compared with what last landed and sent only
when it changed. Saving still comes first in `settle`, so the board has the
new mapping before it hears the new text.
`a_selected_segments_chase_follows_its_count` presses `set-count` on the
choker's P (13 → 16) and reads the red tail at lamp 15 from the board's own
frame; it failed on main.
