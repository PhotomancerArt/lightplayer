---
status: fixed
found: 2026-10-28      # how: the mapping rebuild's G3 walk (M4 P08), Studio's Graph on an emulated C6
fixed: this change     # lands with the mapping rebuild's integration branch
area: lpa-studio-core — the Graph's Debug writes to the board (`ProjectController::apply_patch_pulse` / `apply_fixture_selection`) over a node file the Graph wrote as a body
class: state-conflation
related:
  - lp-app/lpa-studio-core/src/app/project/project_controller.rs (holds_pending_body)
  - lp-app/lpa-studio-core/src/app/graph/graph_controller.rs (save, highlight)
  - lp-core/lpc-model/src/project/overlay/artifact_overlay.rs (put_slot_edit)
  - lp-app/lpa-studio-core/src/app/studio/studio_graph_journeys_tests.rs
  - projects/test/circular-display-walk
  - lp2025/2026-10-27-0114-mapping-rebuild (M4 P07 naming, P08 walk)
---
# The Graph names an output, and the board keeps it unnamed

**Symptom** — On an emulated C6 (`just studio-dev-emu`, board `c6-a`, the
walk project `projects/test/circular-display-walk` uploaded by `lp-cli
upload`), mapping the circular display from nothing in Studio's Graph and
choosing the fixture's start "D0 of output" names the output `1`, as lean
Q96 says: the inspector reads "Output: output 1" and every ring lands. The
header shows two unsaved edits. After **Save** (or without it) and a fresh
page, the Graph opens the board's project with the circle, its rings, ring
3 reversed and its override on D1 — the map2d was kept — but the output has
**no name**: ring 3 reads "Output: 1, which isn't there", the fixture "⚠ 2
problems", and the board's `output.json` (read on the wire, in the board's
console log) has no `"name"`. Before Save the header said 2 unsaved edits;
after a fresh page without Save, 1. The walk's log shows the moment: the
problems appear at step a13, when the output is clicked in the tree, before
Save is pressed.

**Root cause** — The Graph writes a node file it changes (the output's
`name`) as a whole-body edit (`SetArtifactBody`). The board's overlay holds,
per file, either slot edits or a body, never both
(`ArtifactOverlay::put_slot_edit` turns an `Asset` entry into a `Slot` one).
Selecting the output on the Graph writes its `highlight` Debug slot (the
board breathes it), a `PutSlotEdit` on the same file: the body, and the
name in it, was replaced by the highlight; deselecting cleared the highlight
and with it the file's whole overlay entry. Nothing was left to save. Studio
kept showing the name, because its own pending copy
(`asset_edit_buffer`) still held the body and counted it dirty — which is
also why the in-process journey passed: it read Studio's copy and never
selected the output. A fixture's file is hit the same way through its
`selection` Debug slot (a colour order the Graph wrote, lost on a click).
Not device-specific: the in-process board loses it identically once the
output is selected.

**Fix** — `ProjectController::holds_pending_body`: a Debug write
(`apply_patch_pulse`'s `highlight`, `apply_fixture_selection`'s `selection`)
skips a node whose file holds a pending body, on the board (the mirrored
overlay) or in flight (Studio's buffer). The user's change wins; that node
doesn't breathe or chase on the board until Save puts its file on disk,
after which it does again. A highlight already set when the Graph writes a
body is replaced by the body on the board, and its later Clear is a no-op on
a body entry, so that order was already safe.

**Regression coverage** — `a_named_output_keeps_its_name_on_the_board_when_selected_and_saved`
(the output named, selected and deselected twice, the board's overlay
checked each time, Save, the board's disk checked; then selected again it
breathes, a port's lamps written while it does are kept and saved) and
`a_fixtures_colour_order_stays_on_the_board_when_selected`, both in
`studio_graph_journeys_tests.rs`, asking the in-process `lpa-server`
(`Graph::board_text` / `board_saved_text`) rather than Studio's copy. Both
fail without the fix (`the output selected, the board still holds the name
— left: None, right: Some("1")`; `left: Some("rgb"), right: Some("grb")`).
The emulated-C6 walk was not re-run.

**Lesson** — A test that reads what Studio shows cannot see a write the
board dropped: Studio's pending copy outlives the board's. Ask the board.
And transient writes (Debug slots) share the overlay entry with the user's
durable edits; anything that writes a slot to a file must know whether
that file holds a body.
