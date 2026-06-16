# Milestone 5: SyncResult Engine Policy

## Title and goal

Wire **`Engine::handle_fs_changes`** and **commit** to **`registry.sync()`**;
apply [engine-policy-v1](../2026-05-21-artifact-routed-file-reload/m4-fs-change-semantics-harness/engine-policy-v1.md).

## Suggested plan location

`docs/roadmaps/2026-05-21-engine-registry-cutover/m5-sync-result-engine-policy/`

## Scope

**In:** `SyncResult` → node add/remove/refresh; shader/fixture **`SourceFileRef`**
materialize; client commit triggers engine refresh.

**Out:** Server watcher routing (M7); graph-level child list (M6).

## Deliverables

- Fs-change + commit integration tests on engine path.
- M4 harness scenarios re-run on production stack.

## Dependencies

- M4 complete.

## Execution strategy

**Full plan** — policy table + node kind matrix.
