# Milestone 5: Generic View And Debug UI

## Title And Goal

Render watched slot roots and resource previews through generic client/view code.

## Suggested Plan Location

`docs/roadmaps/2026-05-06-slot-domain-cutover/m5-generic-view-debug-ui/`

## Scope

In scope:

- Make `ProjectView` surface slot roots, shape metadata, pending watch state, and resource summaries in a UI-friendly way.
- Replace node-specific detail panels with a generic slot tree renderer where possible.
- Keep small node-specific preview helpers only where they demonstrate resource payloads, such as texture/fixture previews.
- Add UI controls for watching/unwatching conventional state roots.
- Add UI controls for requesting resource payload bytes only when needed.
- Show pending/error state for watched roots and resource payload loading.

Out of scope:

- Polished product UI.
- Full editing/mutation UX.
- Advanced layout/design-system work.

## Key Decisions

- The debug UI is allowed to stay practical and plain, but it should prove the generic model.
- Resource metadata is visible before bytes are fetched.
- Node-specific rendering should be the exception, not the normal path.

## Deliverables

- Generic egui slot tree component.
- Debug UI watch controls based on slot roots rather than legacy node detail.
- Resource skeleton rows and opt-in payload previews.
- Manual or automated smoke checks showing source/state/params/output roots.

## Dependencies

- Milestone 3 project slot sync bridge.
- Milestone 4 runtime node slot roots.

## Execution Strategy

Full plan. Even though the UI is throwaway, it is the proof that the domain model works end to end.

