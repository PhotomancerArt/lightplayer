# Milestone 5: Generic Debug UI Rebuild

## Title And Goal

Rebuild the dev/debug UI as a generic slot and resource inspector over canonical project view state.

## Suggested Plan Location

`docs/roadmaps/2026-05-06-slot-domain-cutover/m5-generic-debug-ui-rebuild/`

## Scope

In scope:

- Render watched slot roots from `SlotMirrorView` using `SlotShapeRegistry`.
- Add watch controls based on conventional slot roots (`source`, `state`, `params`, `output`).
- Render resource skeletons/metadata by default.
- Add explicit UI interest for raw texture/buffer payload previews.
- Provide enough metadata rendering to demonstrate the slot model works with real source defs.

Out of scope:

- Final product UI design.
- Node-specific editor panels unless they are thin helpers over generic slot data.
- Full client-driven mutation UI.

## Key Decisions

- The debug UI is proof of the generic model, not a polished final editor.
- Resource payloads stay opt-in from the UI.
- If a node-specific control is added, it must not reintroduce node-specific wire/state shapes.

## Deliverables

- Debug UI no longer depends on legacy node detail state.
- Watched source roots render generically.
- Resource metadata and selected payload previews are visible.
- Tests or manual evidence notes document the canonical UI sync path.

## Dependencies

- Milestone 4 project view rebuild.

## Execution Strategy

Full plan. Keep the UI utilitarian and direct; the point is to prove the data model, not design the final control surface.
