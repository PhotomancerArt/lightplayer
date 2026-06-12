# ADR 2026-06-12: Incremental Runtime Apply

## Status

Accepted

## Context

M4 moved project edits onto `ProjectRegistry` overlays, but the server bridge
still rebuilt the whole `Engine` after accepted overlay mutations and
filesystem refreshes. That made the new registry API usable, but it kept the
old runtime lifecycle behavior: any project change dropped and recreated the
runtime projection.

That is too coarse for the engine cutover. Runtime node identity, runtime
buffers, output sinks, resolver state, and compiled shader state are expensive
on ESP32. Plain authored value changes should not churn runtime nodes. The
registry already computes effective project changes, so the engine can consume
those summaries directly.

Commit is a separate concern. Overlay mutation makes the effective project live;
commit only persists that already-effective state into durable artifacts.

## Decision

`ProjectRegistry` remains the project truth. It owns artifacts, the overlay, and
the effective `ProjectInventory`.

`Engine` owns the runtime projection. It applies `ProjectChangeSummary` in
place through `Engine::apply_project_changes`.

`ProjectChangeSummary` includes:

- node-definition changes;
- asset changes;
- node-use changes through `NodeUseChangeSummary`.

Incremental runtime apply treats node-use changes and node-definition
kind/error transitions as lifecycle/topology signals:

- removed node uses remove runtime subtrees;
- added node uses create missing runtime spine nodes and attach runtime
  payloads;
- changed node uses are conservatively reprojection candidates;
- definition kind changes and loaded/error transitions reproject affected uses.

Same-kind `NodeDef` body changes are not lifecycle signals. Asset body changes
are not generic engine lifecycle signals. Runtime nodes are responsible for
observing every value they consume through resolver/revision-aware APIs, as
documented in
`docs/adr/2026-06-12-node-runtime-slot-value-contract.md`.

Overlay commit does not apply runtime changes and does not rebuild the engine.
After a successful commit, the server advances the project filesystem version
past commit-origin writes so the next filesystem refresh does not reprocess the
same self-originated changes.

Full project reload remains available as an explicit server-level recovery or
manual operation. It is not the normal edit, refresh, or commit hot path.

## Consequences

The server project wrapper owns both registry and engine and orchestrates:

```text
overlay mutation -> registry change summary -> engine incremental apply
filesystem refresh -> registry change summary -> engine incremental apply
overlay commit -> artifact writes only
manual reload -> rebuild registry and engine from durable artifacts
```

Runtime node ids survive ordinary authored body/value edits. Structural edits
can still remove and reproject affected subtrees.

Engine removal is now responsible for lifecycle cleanup before tree tombstones:

- `NodeRuntime::destroy`;
- demand roots;
- output sinks;
- node-owned runtime buffers;
- project-runtime indexes;
- binding indexes.

The first incremental apply implementation remains conservative. It may rebuild
affected structural subtrees, especially around playlist topology, but it does
not rebuild unrelated runtime nodes or use `NodeDefChangeKind::Body` as a broad
rebuild trigger.

Future work can add finer node-specific reload hooks or structural metadata, but
those should refine this boundary rather than restore full-engine rebuilds to
the edit hot path.
