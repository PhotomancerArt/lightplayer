# ADR: Graph — the mapping lab's canvas is Studio's project canvas

- **Status:** Accepted (G3 passed by Yona, 2026-10-28: "it looks pretty
  good"; small UX refinements wait for the fixture and shader space theory
  session)
- **Date:** 2026-10-28
- **Deciders:** Yona (G3), Photomancer
- **Plan:** `lp2025/2026-10-27-0114-mapping-rebuild` (M4; `notes.md` "M4
  planning", Q71–Q98, C27–C39), landing with M3 in PR #1319. Its
  predecessor, `lp2025/2026-10-17-0202-mapping-lab-into-studio`, planned
  this retirement as its M6 and never ran it.
- **Supersedes:** `2026-08-13-one-project-canvas.md` (the merged
  Map/Patch canvas, its dive, its gesture stack and toolbar).
- **Supersedes in part:** `2026-08-13-editor-meta-and-unified-editor.md`,
  `2026-08-12-studio-workbench-panel-dock.md`,
  `2026-08-27-one-selection-one-tree.md`,
  `2026-08-20-patching-view-grain-follows-activity.md`,
  `2026-08-20-walk-up-assignment-selection-model.md`,
  `2026-07-28-standalone-editor-module.md` (what each keeps is below).
- **Superseded by:** None
- **Related:** `2026-10-27-mapping-model-parts-segments-patcher.md` (the
  model, the patcher and format 6; its 2026-10-28 amendment "Studio's side"
  reads with this), `2026-10-01-agentic-control-offers-in-core.md` and
  `2026-10-02-board-ids-and-typed-offer-parameters.md` (offers with typed
  params), `2026-10-28-project-format-policy.md`.

## Context

Studio had two mapping editors. The **Map** tab (`lpa-mapping-editor`, web
`editor_shell/`) drew a fixture's map2d document (formats 1 to 5: objects
whose order was the wiring order) and arranged fixtures on one canvas; the
**Patch** tab put those objects on output ports through a separate patch
document. Beside them the **mapping lab** (`lab/mapping/`) grew a third
canvas, the Graph: every node a card, fixtures drawn in place, coloured by
the board, its own fixture model and its own page. Studio mounted the lab's
canvas as its Graph tab in 2026-10.

The mapping rebuild replaced both documents with one model (parts render a
tree of segments, a patcher gives each segment its address, map2d format 6
holds both) and moved every project onto it (project format 12). The Map
tab could not read format 6, the Patch tab's file no longer existed, and
the lab's own fixture model was a second copy of what `lpc-mapping` now
is. One editor had to remain.

## Decision

1. **The Graph is Studio's one project canvas and its one mapping editor.**
   The project's tabs are Nodes · Graph. The Map tab, the Patch tab and
   `lpa-mapping-editor` are deleted. `/mapping`, `/map`, `/patch` and
   `/patching` open the Graph and write `/graph`. The fixture face's pencil
   and its Mapped shape tile are a plain link to the Graph ("Edit mapping"),
   with no selection by URL.

2. **The lab's three-part structure stays; the crates are product crates
   in `lp-app/`.**
   - `lpa-graph` (was `lab-mapping-model`) decides: the project's nodes and
     fixtures, selection, verbs, hints, problems' words, the inspector's
     rows, the keyboard. Its statements are its tests.
   - `lpa-graph-web` (was the library half of `lab-mapping-web`) draws what
     `lpa-graph` decided and holds nothing of its own beyond view state.
   - `lpa-graph-files` (was `lab-mapping-files`) opens and saves real
     projects in the product's formats.
   - `lpa-graph-layout` and `lpa-graph-arrows` (were `lab-layout`,
     `lab-arrows`) are pure: a graph in, placements out; an arrow in,
     geometry out. Neither has a dependency.
   - Studio core keeps the controller, the offers, the board highlight and
     the opening (`lpa-studio-core/src/app/graph/`); Studio web mounts
     `lpa-graph-web` (`workbench/graph_view.rs`).
   The four non-web crates are default workspace members, so CI's ordinary
   `cargo test` runs their statements.

3. **One model, one patcher, shared with the engine.** The Graph's fixtures
   are `lpc-mapping`'s `Mapping` and its part tree; the patching it shows is
   `patch_mapping` over port tables cut by the engine's own walk
   (`lpc_engine::node::output_port_slices`), so an address on the Graph and
   the wire it lands on cannot disagree. Edits that rewrite references
   (rename, repeat, unrepeat, expand, counts, rings, overrides, starts and
   flows) live in `lpc-mapping`'s `edit` module, behind an `edit` feature
   that `lpa-graph` enables and no firmware does. The Graph's oracle is the
   landing goldens the format-12 upgrade is held to: a statement opens every
   repo project in the Graph's model and asserts each lamp lands where its
   golden says.

4. **Every edit is a verb, offered at `project/graph/<verb>`.** A verb that
   acts on something takes a `subject` (a tree id, preselected to the
   selection) and typed params (port and flow as choices, lamps and names as
   text, off as a toggle). The inspector's rows, the keys, ⌘K and the app
   agent press the same offers. A verb is offered whenever something can
   take it, never because of the selection. Levels: what takes something
   away (delete, turn off, everything off, back to the flow, remove
   override, unrepeat, remove ring, a count that drops copies an override is
   on) is Undoable; the rest is Routine; nothing is Lasting, because the
   Graph's own undo puts each back. Pointer gestures (drawing, dragging) and
   moving the view stay input and view state.

5. **The lab is retired.** Its page, its recipe and its simulated-board
   page pieces are deleted; its projects are test projects
   (`projects/test/`), its radiance-dome generator is
   `tools/lpt-radiance-gen`. The design surface is Studio's Graph: its
   stories, `just studio-dev` (with `?emu=tab` for a board), and the
   crates' statements. UX directions go through `yona-ux` spikes.

## Consequences

- Studio has one mapping editor to keep honest instead of three, and the
  editor reads and writes exactly what the engine runs.
- The agent sees the Graph through the offer tree like any surface; its
  system prompt names `project/graph/<verb>` and no longer teaches the
  arrange or patch offers.
- `editor.json` keeps one live surface, the Graph's `canvas` (pinned
  positions). Placements the Map tab wrote under `mapping` are kept unread,
  as any unknown surface is.
- The Nodes view's Outputs panel and the node-card patch bay still draw
  patching from older derivations (`ui_patch_surface.rs`,
  `patch_bay_derivation.rs`); drawing them from the patcher as rows of
  spans is future work.
- `Map2dDoc` (formats 1 to 5) stays in `lpc-mapping`: its shapes and
  resolver are the parts' geometry, two generators build geometry with it
  before writing format 6, and the upgrade's statements read old documents
  with it.
- The Graph's canvas is still built from `lpa-graph`'s own node graph
  (nodes, channels, bindings, outputs), not from Studio's project model;
  only the fixture mapping domain was replaced by `lpc-mapping`. Building
  the canvas from Studio's model is future work.
- Two presentations of where a value came from (A: each value says its
  source; B: the flow's start as a quiet line, then only what is set here)
  remain behind the Graph's Debug menu until Yona picks one.

## Alternatives Considered

- **Fold the Graph into `lpa-studio-core` and `lpa-studio-web`.** One fewer
  set of crates, but the statements would run inside Studio's slowest test
  binary, `project_controller.rs` is already its largest file, and the
  p2/p3 split already existed. Moving with `git mv` kept history.
- **Keep the lab as a separate design surface.** It would keep a second
  model alive beside `lpc-mapping` (the drift the rebuild ended) and a page
  only one person opened. Studio's stories and an emulated board give the
  same loop on the real model.
- **Port the Map tab to format 6.** It edited documents whose order was the
  wiring order; format 6 separates layout from patching, so the port would
  have been a rewrite of the editor the Graph already is.

## Follow-ups

- Pick presentation A or B and delete the other (Q98), after the fixture
  and shader space theory session.
- The fixture's rectangles and coordinate spaces (fixture box, drawing
  extent, canvas, sampled picture) need their own theory session before any
  plan (Yona at G3).
- The Outputs panel and the patch bay as rows of spans from the patcher.
- The Graph's canvas built from Studio's project model.

## Amendment, 2026-10-29: the fixture card and one camera

Fixture and shader space (`2026-10-29-fixture-and-shader-space.md`) settles the fixture's rectangles:

- **One camera.** You zoom and pan the Graph. A fixture card is a placement
  inside it, with no zoom or pan of its own.
- **A standard width.** A fixture's card is 240 px wide at zoom 1. Its
  height follows its box's ratio, capped at 3× the width, with a minimum
  set by the chrome. There is no resize gesture and no corner drag. The box
  is set in the card's Box section, with "Fit the box to the parts".
- **Its own space.** A fixture is drawn in its own coordinate system, with
  rulers in its units. The picture under its lamps is the source sampled
  through the same transform as the lamps, faded past the box. The card is
  drawn in `docs/ux/anatomy/fixture-card.md`.
- The follow-up "the fixture's rectangles and coordinate spaces need their
  own theory session" is closed.
