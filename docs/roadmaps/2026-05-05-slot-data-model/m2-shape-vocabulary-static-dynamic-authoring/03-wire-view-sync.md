# Phase 3: Wire And View Sync

## Scope Of Phase

Prove shape-aware sync from runtime/source slot access into a client-side generic mirror.

In scope:

- full sync of registry and root snapshots,
- incremental diffs for leaf and container structural changes,
- view-side patch application using shapes rather than hardcoded field indexes,
- tests for pruning stale map/enum/option data.

Out of scope:

- real message API integration,
- artifact mutation,
- resource sync cleanup.

## Code Organization Reminders

- Keep patch collection in `wire`.
- Keep patch application in `view`.
- Keep generic traversal based on `SlotShapeRegistry` and `SlotDataAccess`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.

## Implementation Details

- `wire::full_sync` sends the shape registry snapshot and generic root snapshots.
- `wire::collect_diff` emits replace patches at the changed slot boundary.
- `view::MockClient` stores `SlotData` plus shape ids and applies patches by resolving record fields through the registry.
- Tests must cover:
  - static Rust structs traversed through `SlotAccess`,
  - typed source map key changes,
  - dynamic shader params,
  - enum switch pruning stale variant data,
  - option `Some -> None`,
  - shape refs for shader param value shapes.

## Validate

```bash
cargo test -p lpc-slot-mockup
```
