# Milestone 3: Canonical Project Sync Rebuild

## Title And Goal

Define and implement the slot-first canonical project sync protocol.

## Suggested Plan Location

`docs/roadmaps/2026-05-06-slot-domain-cutover/m3-canonical-project-sync-rebuild/`

## Scope

In scope:

- Add canonical project sync request/response types outside legacy modules.
- Model node lifecycle/status changes without legacy node detail payloads.
- Include slot registry snapshots/changes and watched slot root full snapshots/patches.
- Carry resource summaries and explicit resource payload responses in the canonical protocol.
- Wire `CoreProjectRuntime::get_changes()` or its successor to produce canonical sync responses for source roots first.
- Add tests for initial full sync, incremental source root diffs, registry updates, map key removal, and client-prunable payloads where applicable.

Out of scope:

- Runtime node state/params/output roots except minimal placeholders if needed.
- Generic debug UI.
- Client-driven mutation.
- Reintroducing legacy detail compatibility.

## Key Decisions

- Canonical messages are not renamed legacy messages. They should be designed around frame, node lifecycle, slot registry/root data, and resources.
- Slot sync is primary in the new protocol, not an auxiliary field hidden inside old detail responses.
- Resource bytes remain explicit opt-in payloads.

## Deliverables

- `lpc-wire` contains canonical project sync request/response types.
- `lpc-engine` can produce canonical sync over real project/source slot roots.
- Tests prove canonical source-root full sync and incremental diff behavior.
- No active code path requires legacy project response/detail types.

## Dependencies

- Milestone 2.2 legacy project sync demolition.
- Milestone 2 source slot roots.

## Execution Strategy

Full plan. This milestone rebuilds the protocol boundary and should keep implementation slices reviewable: wire shape first, engine projection second, tests throughout.
