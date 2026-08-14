# ADR: The one project canvas

- **Status:** Accepted
- **Date:** 2026-08-13
- **Deciders:** Photomancer
- **Plan:** lp2025/2026-08-13-0859-one-project-canvas (PR #416);
  successor to the unified editor shell (PR #415).

## Context

The workbench Mapping view carried two canvas implementations: the
arrange-level `arrange_canvas.rs` in the shell (SVG `viewBox` camera,
fixture frames, drag-to-arrange) and the tool-bearing editor canvas in
`lpa-mapping-editor` (`Camera{x,y,scale}`, lamps, tools, session undo).
Double-clicking a fixture swapped components, snapped the camera to the
fixture, and swapped the chrome. The #415 gate ruling: "I would expect
double clicking not to jump to a new editor but to start editing in
place. same canvas, same feeling… the best way to keep them in sync is
to use the same code."

## Decision

**One canvas, three cuts.**

1. **Crate vs shell = surface vs policy.** The merged canvas lives in
   `lpa-mapping-editor`, which stays project-unaware: geometry, camera,
   gesture grammar, tools, per-document session/undo, the fixture
   layer, and the placement seam. Fixture data enters as plain props
   (`FixtureSprite`: label, color, placement, bounds, body, selection
   flags); intent leaves as events (`FixtureEvent::{Select, Move,
   Dive}`). The shell keeps policy: `EditorMetaOp` dispatch,
   drag-override-until-echo, pack slots, flag-driven prefetch, journal
   stamping, and the asset pipeline.

2. **The dive is layer state, not component identity.** The canvas is a
   z-stack: dot grid (project space) → fixture sprites (project space)
   → doc layers (only when dived: the live session's lamps, wiring,
   handles, marquee — inside one nested camera ∘ placement SVG group)
   → floating overlays. Diving swaps the focused fixture's static body
   for the live doc layers in its exact placement and dims the
   neighbours to context opacity. **The camera never moves on
   enter/exit/switch** — the zoom float's fit frames the focused
   fixture on demand instead (the "snap viewport to fixture"
   affordance).

3. **The placement seam is view-layer only.** `Placement` (translate ∘
   rotate ∘ uniform scale, f64) renders doc space inside project space;
   pointer math routes through `placement.inverse(camera.view_to_doc)`;
   every screen-constant size divides by the EFFECTIVE scale
   (`camera.scale × placement.s`), and the camera clamp (0.02–64)
   bounds effective zoom so shrunken placements still reach lamp-editing
   size. `MapEditorSession` and the document stay doc-space untouched —
   the old `ContextFixture` inversion (neighbours transformed into the
   focused doc's space, camera snapped) is deleted outright.

4. **One gesture stack, mode-scoped at the top.** Element-level
   handlers (lamps, hitlines, handles, vertices) carry the editor
   grammar when dived; a single canvas-level hit test (SVG child
   delegation is unreliable under Dioxus, 07a39242f) carries the
   fixture grammar when not: press-drag = move (one committed op per
   gesture), tap = select, double-click = dive — and while dived, a
   neighbour double-click SWITCHES the dive (journal `NodeSwitch`).
   Esc walks the editor ladder and its last rung exits the dive. The
   wheel grammar is shared throughout.

5. **The toolbar is data.** One strip component renders
   `ToolbarGroup`s of `ToolbarItem`s and dispatches clicks by id; the
   fixture ↔ mapping morph is a swap of item lists, one chrome row
   total. The R5 patching view adds its verbs as another list, never
   another strip.

6. **The asset pipeline is a non-visual coordinator.** Fetch → seed the
   workbench-owned session (parsed-doc echo suppression with a bounded
   queue of in-flight applies, so rapid commits' out-of-order echoes
   never re-seed) → apply on commit bumps (canvas gestures, editor
   keys, and Props-pane edits share one bump counter).
   Refuse-don't-rewrite holds: an unreadable body renders a refusal
   banner over a still-visible sprite, tools disabled, file untouched.

## Consequences

- Fixture arranging and lamp editing are the same surface; camera
  continuity makes the dive feel like staying (G1 ruling: "this feels
  exactly right").
- The R5 patching view reuses the contract: same canvas, different
  furniture (its own sprite list + toolbar items), by construction.
- Rotation quirks accepted for v1 (doc-space marquee renders rotated;
  wiring numbers rotate with the doc) —
  `docs/debt/placement-rotation-quirks.md`.
- Parked later work, deliberately not precluded: snap-viewport /
  isolation mode / viewport rotation (spike candidates), per-view
  camera memory across tab switches, multi-select across fixtures.
- Deleted: `arrange_canvas.rs` (viewBox camera model), `MapEditor`,
  `EditorHeader`, `ContextFixture`/`dive_context` — the crate exports
  composable parts (canvas, floats, keys, hint) instead of a wrapper
  editor component.
