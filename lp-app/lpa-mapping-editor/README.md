# lpa-mapping-editor

The 2D mapping editor: the ONE project canvas and the editing grammar
over `lpc-mapping` documents, mounted by the Studio workbench's Mapping
view. Also the home of the shared lamp-view geometry (wiring-arrow
segments, neutral lamp fill) used by both the canvas and Studio's
fixture renderers.

ADRs: `docs/adr/2026-07-28-standalone-editor-module.md` (the carve-out),
`docs/adr/2026-08-13-one-project-canvas.md` (the merged canvas),
`docs/adr/2026-08-27-one-selection-one-tree.md` (the one selection: the
dive as derived scope, the fixture-grain multi-select grammar, and the
view-parameterized pick grain).

## Boundary

**Data in, events out — the crate is the SURFACE; the host keeps the
POLICY.** This crate knows the mapping document schema (`lpc-mapping`)
and how to edit it, and it knows how to render a project's fixtures as
sprites. It has **no** knowledge of projects, assets, routes, or the
Studio server:

- Fixtures enter as plain `FixtureSprite` props (label, color,
  placement, own-space bounds, one of three honest bodies, selection
  flags) and fixture intent leaves as `FixtureEvent`s (tap =
  `Select { pick, toggle }` with shift as the sibling toggle, background
  band = `Marquee`, press-drag = `Move { moves }` carrying the WHOLE
  selected set with one committed gesture — the shared selection box's
  corner handles scale the set uniformly when the host asks for
  `transform_handles` — and double-click = `Dive` carrying the clicked
  pick). Right/middle-drag and space+drag pan. The host owns the
  override lifecycle, packing, persistence, journal stamping, and WHAT a
  pick selects (fixture grain or object grain is view policy).
- The DIVE is layer state on the same canvas: the host passes the
  focused sprite's key, the live `MapEditorSession`, and the fixture's
  `Placement`; the doc layers render inside one nested camera ∘
  placement SVG group and pointer math routes through the placement
  inverse. The session and document never learn they are placed — the
  seam is view-layer only. The camera is the host's signal and nothing
  in the crate moves it on dive.
- There is no wrapper editor component: hosts compose `EditorCanvas`
  with `ZoomFloat`, `HelpFloat`, `tool_hint`, and the keyboard grammar
  `handle_editor_key` (whose esc ladder ends in
  `EditorKeyOutcome::ExitDive` — leaving the dive is the host's rung).

## Layout

- `editor_core/` — pure Rust, host-tested, no Dioxus:
  - `editor_session.rs` — working doc + gesture-coalesced JSON-snapshot
    undo + selection + tool state. Every mutation is a session op; drags
    re-derive from the gesture snapshot (no drift; one gesture = one
    undo step); edits sanitize so every produced document resolves.
  - `map_selection.rs` / `map_tool.rs` — tree-path selection and the
    tool enum (select / grid / ring / path-with-draft /
    polygon-with-draft-and-population).
  - `camera.rs` — pan/zoom/fit math; the clamp bounds EFFECTIVE zoom
    (camera × placement scale).
  - `placement.rs` — translate ∘ rotate ∘ uniform-scale with forward /
    inverse transforms; the seam's math.
  - `doc_fit.rs` — what "zoom to fit" frames for a document.
  - `view_geometry.rs` — shared lamp-view primitives behind neutral
    inputs.
- `view/` — Dioxus components, thin over the core:
  - `canvas/` — `EditorCanvas` (pointer routing, camera + placement
    groups) over `layers/` (`fixtures`, `doc`, `selection`, `draft`,
    `marquee`) plus `canvas_anchor`, `palette`, `lamp_metrics`,
    `live_fills` (live colors are direct DOM writes — a 60Hz feed costs
    zero VDOM work).
  - `floats.rs` (zoom + help + tool hint), `keys.rs` (the keyboard
    grammar), `view_options.rs`, `object_properties.rs`, `reference.rs`,
    `wheel.rs` (the house wheel grammar: scroll pans, ⌘scroll zooms).

## Interaction grammar

Tools V/G/R/P/O (select / grid / ring / path / polygon). Creation drops
a default object and opens its properties; the path tool previews
resolved lamps and the chain link live, Enter/double-click finishes,
Escape backs out one vertex. The polygon tool draws a closed OUTLINE and
carries a population mode — **outline** (lamps ride the perimeter,
`Polygon`) or **filled** (a lamp lattice inside it, `FilledPolygon` —
the shaped matrix). Its ghosts come from the real resolver, so the
preview is cell-for-cell what closing commits; click the first corner or
Enter closes. The mode is switchable after the fact from the properties
pane: the outline is the authored thing, the population is how light
fills it. Selection is **tree-path based** (`ShapePath` +
`MapSelection`): double-click descends into a group, edits through a
descended path write through to the authored shape (every repeat
instance follows) — rationale:
`docs/adr/2026-08-05-map2d-editor-selection-tree-model.md`. The
properties pane renders the selected path as a STACK of editable cards,
deepest first (the B′ ruling amending that ADR) — the host shell
composes its own placement card and context strip around it. Click,
shift-click, marquee, ⌘A select; corner handles resize uniformly;
outline vertices drag (path, polygon, filled polygon); double-clicking
an edge inserts a corner and Delete removes the selected one, down to
each shape's floor (a run keeps 2, an outline 3) — with no vertex
selected, Delete removes the object. The esc ladder: draft backout →
drop vertex → ascend group → reset tool → exit the dive. ⌘Z/⇧⌘Z undo/redo; 0 fits. View toggles N/A/L/F: wiring numbers,
direction arrows, live output colors, texture-frame fit preview.

At the fixture grain (not dived): tap selects, press-drag moves
(4px CSS threshold before a press becomes a drag), background tap
deselects, double-click dives; while dived, neighbour sprites dim and
answer only double-click (the dive-switch).

## Documents

`*.map2d.json` (`lpc-mapping` schema, format-versioned): parametric
grid / multi-ring circle / path objects; object order is wiring order.
Rings auto-space from the outer radius; per-ring counts can override
the circumference-derived defaults. The SVG importer
(`lpc-mapping::import`) rejects curve commands (`UnsupportedCommand`) —
it imports the straight-line subset only.
