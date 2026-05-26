# Milestone 6: Graph Reconciliation

## Title and goal

When **`project.toml`** / playlist invocation wiring changes, engine mutates the
node tree (add/remove/repoint) — not only in-def slot patches.

Promoted from [artifact-routed M8](../2026-05-21-artifact-routed-file-reload/m8-project-graph-reconciliation.md).

## Suggested plan location

`docs/roadmaps/2026-05-21-engine-registry-cutover/m6-graph-reconciliation/`

## Scope

**In:** Detect invocation graph diffs from `SyncResult`; engine child attach/detach;
tests for top-level and playlist inline children.

**Out:** Minimal diff optimality for every edit shape.

## Dependencies

- M5 complete.

## Execution strategy

**Full plan**
