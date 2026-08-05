# lpa-mapping-editor

The 2D mapping editor: a standalone, project-agnostic editor for
`lpc-mapping` documents, served at Studio's `#/mapping` route and (roadmap
M5) mounted on the fixture face. Also the home of the shared lamp-view
geometry (universe palette/derivation, wiring-arrow segments) used by both
the editor canvas and Studio's fixture-face renderer.

ADR: `docs/adr/2026-07-28-standalone-editor-module.md`.

## Boundary

Document in, edits out. This crate knows the mapping document schema
(`lpc-mapping`) and how to edit it. It has **no** knowledge of projects,
assets, routes, or the Studio server — hosts own persistence:

- `MapEditorPage` (the `#/mapping` route body) adds file open (picker +
  drag-and-drop), save (data-URL download), and localStorage autosave —
  plus the reference tracing image (`ReferenceImage`), persisted under
  its own localStorage key so tracing state can never touch the document
  autosave. The reference never enters the map2d document (10KB asset
  budget; reference art is routinely larger).
- `MapEditor` is the embeddable seam: `doc` + `doc_epoch` in,
  `on_doc_change(json)` out on every committed change. The fixture face
  mounts it and syncs via the asset pipeline's whole-body apply. Two
  optional host hooks serve that embed: `shared_view` (a host-owned
  `Signal<EditorViewOptions>` — the face's toggle bar drives the canvas
  layers and the header hides its own view cluster) and `live_colors`
  (per-wiring-index lamp colors; rendered when the `live` view option
  is on, so the editing surface can show real engine output).

This is deliberate Studio modularization: the first Dioxus
component-library crate carved out of the app, and the template for the
next ones.

## Layout

- `editor_core/` — pure Rust, host-tested, no Dioxus:
  - `editor_session.rs` — working doc + gesture-coalesced JSON-snapshot
    undo + selection + tool state. Every mutation is a session op; drags
    re-derive from the gesture snapshot (no drift; one gesture = one undo
    step); edits sanitize so every produced document resolves. Includes
    Illustrator-style `expand_object` (parametric → plain path, same
    lamps).
  - `map_selection.rs` / `map_tool.rs` — selection (object indices +
    vertex; remapped on structural edits) and the tool enum
    (select / grid / ring / path-with-draft).
  - `camera.rs` — pan/zoom/fit math over doc space.
  - `view_geometry.rs` — shared lamp-view primitives behind neutral
    inputs (positions + spans in caller view units).
- `view/` — Dioxus components: `MapEditor` (header + canvas + properties
  popover + wiring rail), `MapEditorPage`, `EditorCanvas`,
  `EditorHeader`, `PropertiesPopover`, `ObjectList`.

## Interaction grammar

Tools V/G/R/P (select / grid / ring / path). Creation drops a default
object and opens its properties (no drag-to-size); the path tool previews
resolved lamps and the chain link live, Enter/double-click finishes,
Escape backs out one vertex (never discards wholesale). Selection: click,
shift-click, marquee, ⌘A; corner handles resize uniformly; single-path
vertices drag; Delete removes. Views N/A/U/F: wiring numbers, direction
arrows (gold dashed chain hops between objects), universe colors
(auto-flow, 170 RGB lamps/universe; ranges annotate as `u:lo-hi`),
texture-frame fit preview. ⌘Z/⇧⌘Z undo/redo; 0 fits.

## Documents

`*.map2d.json` (`lpc-mapping` schema, format-versioned): parametric
grid / multi-ring circle / path objects; object order is wiring order;
universes derive from it. Rings auto-space from the outer radius; per-ring
counts can override the circumference-derived defaults. The SVG importer
(`lpc-mapping::import`) flattens curve commands to endpoint lines.
