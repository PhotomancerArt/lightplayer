---
status: open
found: 2026-10-28      # how: the mapping rebuild's G3 walk (M4 P08), Studio's Graph on an emulated C6
area: lpa-studio-core — the Graph's save of a node file it changes (`graph_controller.rs` save → `apply_asset_body`), and the board's overlay / Save of a node definition's body
class: unclassified
related:
  - lp-app/lpa-studio-core/src/app/graph/graph_controller.rs
  - lab/mapping/model/src/domain/output_naming.rs
  - lab/mapping/files/src/save.rs (output_value)
  - lp-app/lpa-studio-core/src/app/studio/studio_graph_journeys_tests.rs (the_circular_display_from_nothing, which passes in process)
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
after a fresh page without Save, 1.

**What is known** — The same presses in process
(`the_circular_display_from_nothing`, an in-process board) see `output.json`
with `"name": "1"` through Studio's asset path, so the Graph does write the
output's file there. What differs on the device path (the overlay of a node
definition's body, its Save, or Studio not sending it) is **not found yet**.

**Workaround** — None in the Graph. A project whose output already has a
name (every repo project, and every project Studio's setup flow makes) is
not affected: naming only happens for an output with no name.
