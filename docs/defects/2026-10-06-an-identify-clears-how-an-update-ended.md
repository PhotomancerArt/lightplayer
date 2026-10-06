---
status: fixed
found: 2026-10-06      # how: e2e (`just walk-ota-emu`, step `cant-get`)
fixed: 2cff12264
area: lpa-devices evidence.rs (`last_update_outcome`)
class: state-conflation
related:
  - docs/defects/2026-10-06-an-updates-markers-drop-when-its-card-merges.md (same step)
---
# An identify clears how an update ended

**Symptom** — Walk step `cant-get` (E13): the no-click restore of a
core-only board found its engine nowhere and ended on "can't get". After
"Set up this device", the kept card said "Restoring firmware…" again
instead of "Needs …, which Studio can't get", and it offered no Install.

**Root cause** — The start of any activity cleared `last_update_outcome`,
including the identify that keeping a board runs. That outcome is what
tells the card *why* an update stopped, and it also blocks a no-click
restart that would only miss again. So an identify erased the one fact the
next row needed.

**Fix** — The outcome is cleared only when firmware is written again
(Update, Flash, Erase).

**Regression coverage** —
`studio_update_e2e_tests::an_engineless_board_whose_engine_is_nowhere_offers_this_studios_build_once_kept`
(both reset modes), and the walk's `cant-get` step.

**Lesson** — Evidence about a *board's firmware* outlives activities that
only look at the board. Clear it on the events that change what it
describes.
