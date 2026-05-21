# Milestone 8: project.toml / Graph Reconciliation

## Title And Goal

Support **`project.toml`** and invocation wiring changes: engine tree mutation
when parent child lists change.

## Suggested Plan Location

`docs/roadmaps/2026-05-21-artifact-routed-file-reload/m8-project-graph-reconciliation/`

## Scope

In scope:

- Detect invocation diffs; engine add/remove/repoint children.
- Tests for add/remove top-level nodes, playlist inline children.
- ChangeSet may express graph-level node add/remove (extends M5).

Out of scope:

- Optimal minimal diff for every edit shape.

## Dependencies

- M7 server wire-up (or M6 engine apply path minimum).

## Execution Strategy

Full plan.

Suggested chat opener:

> M8 graph reconciliation needs a full plan. Agree?
