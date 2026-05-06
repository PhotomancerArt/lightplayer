# Milestone 3: Project Slot Sync Bridge

## Title And Goal

Carry production slot registry snapshots and watched root data over project sync while legacy detail sync still exists.

## Suggested Plan Location

`docs/roadmaps/2026-05-06-slot-domain-cutover/m3-project-slot-sync-bridge/`

## Scope

In scope:

- Extend project sync to include slot registry snapshots, watched root full syncs, and incremental slot patches.
- Integrate `lpc-wire::slot` helpers into `CoreProjectRuntime::get_changes()` or its successor path.
- Update `ProjectView` to own/use `SlotMirrorView` for real project data.
- Preserve legacy `NodeDetail` temporarily as bridge code.
- Add tests for initial full sync and incremental source root diffs.

Out of scope:

- Removing legacy node details.
- Generic debug UI replacement.
- Runtime node state roots unless they are needed as a minimal placeholder.

## Key Decisions

- The bridge is temporary and should be visibly isolated.
- Slot sync rides next to current project sync first, then becomes the primary path later.
- Registry snapshots are authoritative for client-side generic rendering and patch application.

## Deliverables

- Real project responses include slot sync payloads.
- `ProjectView` applies slot sync to a production `SlotMirrorView`.
- Tests cover source root update, registry update, map key removal, and client pruning where applicable.
- Legacy detail sync remains functional during the bridge.

## Dependencies

- Milestone 1 watch request vocabulary.
- Milestone 2 source slot roots.

## Execution Strategy

Full plan. This changes server wire projection and client view state at the same time.

