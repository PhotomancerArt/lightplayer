---
status: fixed
found: 2026-11-12      # how: an emulated walk (`just studio-dev-emu`, board c6-a), after PR #1341
area: lpa-studio-core — the Graph's selection on the board (`GraphController::highlight`)
class: state-conflation
related:
  - lp-app/lpa-studio-core/src/app/graph/graph_controller.rs (highlight)
  - lp-app/lpa-studio-core/src/app/project/project_controller.rs (sweep_board_highlights, apply_fixture_selection, apply_patch_pulse)
  - lp-app/lpa-studio-core/src/app/studio/studio_graph_e2e_tests.rs (a_reloaded_page_clears_the_chase_it_did_not_draw)
---
# A reloaded page leaves the old chase on the board

**Symptom** — With a line selected on the Graph (or a line being drawn),
reloading Studio's page, or opening the board from a second page, leaves
the board showing the old pattern. The new page has nothing selected, yet
the strip keeps chasing.

**Root cause** — The Graph shows its selection through Debug slots: each
fixture's `selection` and each output's `highlight`. Debug slots live in
the board's overlay until the project unloads. Only the page that wrote
them knew to clear them: `ProjectController::selected_fixtures` and
`pulsed_outputs` list what this Studio wrote, and `GraphController`'s
`Shown` what it last sent. A new page starts with all three empty, so with
nothing selected it has nothing to send and nothing to clear. What this
Studio wrote was taken for what the board shows. The overlay mirror the
sync already holds says what the board shows, and nothing read it.

**Fix** — `ProjectController::sweep_board_highlights` runs once per
connection, as soon as the overlay mirror is the board's (the project
read, its runtime status in, no overlay fetch due). It removes every
`selection` and `highlight` slot edit in the overlay in one batch and
empties its own lists; the Graph then forgets what it showed and sends its
selection afresh. It runs from `GraphController::highlight`, which the
actor calls after every batch whether or not the Graph is the place, so a
page that reloads onto the home page clears the board too.
`a_reloaded_page_clears_the_chase_it_did_not_draw` selects a segment in one
Studio, opens a second on the same in-process board with nothing selected,
and reads the overlay and the board's frame; it failed before the fix.
