# Sync Errors Are Lossy

- **Severity:** P1
- **Status:** open
- **First seen:** 2026-06-10-api-design-review.md
- **Last reviewed:** 2026-06-10-api-design-review.md
- **Owner:** unassigned

## Finding

Several filesystem sync and committed-artifact refresh paths collapse errors into no-op/default behavior. That is risky for engine cutover because the engine needs to know when reload failed, which artifact failed, and whether it should keep old runtime state, mark a node error, or rebuild part of the graph.

## Evidence

- `lp-core/lpc-node-registry/src/registry/sync.rs:70` - `sync_fs` wraps filesystem events into `SyncOp::Fs`.
- `lp-core/lpc-node-registry/src/registry/sync.rs:71` - `sync_fs` maps `Ok(outcome)` to committed changes.
- `lp-core/lpc-node-registry/src/registry/sync.rs:73` - any `SyncError` becomes `SyncResult::default()`.
- `lp-core/lpc-node-registry/src/registry/sync.rs:104` - `apply_fs_sync` ignores the result of `reconcile_artifacts()`.
- `lp-core/lpc-node-registry/src/registry/inventory.rs:32` - `sync_def_artifact` derives the new inventory.
- `lp-core/lpc-node-registry/src/registry/inventory.rs:35` - inventory errors are swallowed with `return`.
- `lp-core/lpc-node-registry/src/registry/commit.rs:115` - commit refresh calls `registry.sync_def_artifact(...)`.
- `lp-core/lpc-node-registry/src/registry/commit.rs:118` - that helper returns `Ok(())` regardless of inventory/reconcile failures inside `sync_def_artifact`.
- `lp-core/lpc-node-registry/src/registry/effective_read.rs:113` - committed byte reads collapse any store read error to `Ok(None)`.

## Impact

The registry can silently report "no changes" while failing to reconcile the artifact inventory. For an engine reload path, that can leave stale runtime nodes alive, miss newly invalid definitions, skip child removal/addition, or hide broken references. Parse errors are represented as `NodeDefState::ParseError`, which is good, but structural registry failures such as duplicate definition locations, specifier resolution failures, artifact unregister failures, or missing committed bytes need explicit reporting.

## Suggested Fix

Make the engine-facing sync API explicit about failures.

Recommended direction:

- Change the engine/server path to use `Result<SyncResult, RegistryError>` or `Result<SyncOutcome, SyncError>` instead of `sync_fs`'s lossy convenience.
- Keep a lossy helper only if it is renamed to make the behavior obvious, such as `sync_fs_best_effort`.
- Have `sync_def_artifact` return `Result<(), RegistryError>` and propagate inventory failures to both filesystem sync and commit.
- Include per-artifact error details in `SyncResult` if the desired policy is "continue but report failures".
- Treat missing committed bytes distinctly from "unknown/unregistered path" in effective reads.

## Validation

- Add a filesystem sync test where a child `ref` changes to an invalid specifier and assert the error is visible.
- Add a commit test where a pending TOML edit creates an invalid referenced child and assert commit reports the failure or records a parse/error state.
- Add a test proving `sync_fs` no longer hides `reconcile_artifacts` failure, or explicitly rename and document the lossy helper.

## History

- 2026-06-10: opened by Codex API design review.
