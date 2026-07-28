# lpa-mapping-editor

The 2D mapping editor module: a standalone, project-agnostic editor for
`lpc-mapping` documents, and the home of the shared lamp-view geometry
(universe palette/derivation, wiring-arrow segments) used by both the
editor canvas and Studio's fixture-face renderer.

## Boundary

Document in, edits out. This crate knows the mapping document schema
(`lpc-mapping`) and how to edit it. It has **no** knowledge of projects,
assets, routes, or the Studio server — hosts own persistence:

- The `#/mapping` page (Studio route) wraps the editor with file open/save
  and localStorage autosave.
- The fixture face (roadmap M5) will mount the same embeddable editor and
  sync via the asset pipeline's whole-body apply.

This is deliberate Studio modularization (parent plan D5): the first
Dioxus component-library crate carved out of the app, and the template for
the next ones.

## Layout

- `editor_core/` — pure Rust, host-tested, no Dioxus:
  - `editor_session.rs` — doc + gesture-coalesced JSON-snapshot undo +
    selection + tool state; every mutation flows through session ops
    (`*_from_gesture` ops re-derive from the gesture snapshot so pointer
    streams never accumulate drift). Session ops sanitize edits so every
    produced document resolves.
  - `map_selection.rs` / `map_tool.rs` — selection (object indices +
    vertex; the session remaps on structural edits) and the tool enum
    (select / grid / ring / path-with-draft).
  - `camera.rs` — pan/zoom/fit math over doc space.
  - `view_geometry.rs` — shared lamp-view primitives behind neutral
    inputs (positions + spans in caller view units).
- `view/` — Dioxus components (arrive with the route work): `MapEditor`
  (embeddable) and `MapEditorPage` (standalone chrome + file I/O).

Plan: `2026-07-27-2d-mapping-system/04-standalone-editor/` (P1–P4).
