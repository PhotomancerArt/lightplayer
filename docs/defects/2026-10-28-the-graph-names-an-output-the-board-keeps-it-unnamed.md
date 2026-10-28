---
status: fixed
found: 2026-10-28      # how: the mapping rebuild's G3 walk (M4 P08), Studio's Graph on an emulated C6
fixed: b87770d5d       # lands with the mapping rebuild's integration branch
area: lpa-studio-core — the Graph's Debug writes to the board (`ProjectController::apply_patch_pulse` / `apply_fixture_selection`) over a node file the Graph wrote as a body
class: state-conflation
related:
  - lp-app/lpa-studio-core/src/app/project/project_controller.rs (hold_body_as_slots)
  - lp-app/lpa-studio-core/src/app/project/asset/def_body_slot_edits.rs
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

**Fix** — First (b87770d5d), `ProjectController::holds_pending_body`: a
Debug write skipped a node whose file held a pending body, so the node did
not breathe or chase on the board until Save. That left the general bug: ANY
slot edit to such a file (a value the slot editor sets, the agent's `set`)
still dropped the body. The fix that replaced the guard (M4 P08b, in Studio
core, no firmware change): before any slot edit is sent to a node file
whose body the board holds, `ProjectController::hold_body_as_slots` says
the body as the slot edits that make the file on disk read as it
(`asset/def_body_slot_edits.rs::slot_edits_for_body`, checked by writing
both and comparing). The board then holds the body's change and the slot
edit side by side; Save writes what the body said, and the commit keeps the
Debug edits pending, so a selected node keeps breathing across Save. Studio
reads the file as the body meanwhile (`held_bodies`, re-read from the board's
slot edits when a persisted one lands). The other order is covered too: a
body the Graph writes to a file carrying a Debug slot edit goes as slot
edits with the Debug ones put back (`write_body_beside_debug_slots`). A body
that can't be said as slot edits (a file not on disk yet, a body of another
kind) falls back to the guard's behaviour: the Debug write is skipped, and
a user's slot edit is refused with words that say to save first.

**Regression coverage** — `a_named_output_keeps_its_name_on_the_board_when_selected_and_saved`
(the output named; selected, deselected, selected: it breathes before Save
and the board holds the name each time; Save while selected writes the name
and it still breathes; a port's lamps written while it breathes keep the
highlight, and are saved), `a_fixtures_colour_order_stays_on_the_board_when_selected`
and `a_slot_edit_to_a_file_with_an_unsaved_body_keeps_both_on_the_board`
(a colour order the Graph wrote, then the slot editor's `wire_reversed`:
the board, Studio and the saved file hold both), all in
`studio_graph_journeys_tests.rs`, asking the in-process `lpa-server`
(`Graph::board_text` / `board_saved_text`) rather than Studio's copy. All
three fail with the hold disabled (`the output selected, the board still
holds the name — left: None, right: Some("1")`; `left: Some("rgb"), right:
Some("grb")` twice). The slot-edit statements are
`def_body_slot_edits::tests`.

**Lesson** — A test that reads what Studio shows cannot see a write the
board dropped: Studio's pending copy outlives the board's. Ask the board.
And transient writes (Debug slots) share the overlay entry with the user's
durable edits; anything that writes a slot to a file must know whether
that file holds a body. A guard that fixes the case found (the Debug
write) leaves the class (every slot write): fix where the two forms meet.
