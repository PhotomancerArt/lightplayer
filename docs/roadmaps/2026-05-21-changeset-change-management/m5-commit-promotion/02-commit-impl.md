# Phase 02 — `commit()` Implementation

**Dispatch:** sub-agent: main | parallel: -

## Scope of phase

Implement `NodeDefRegistry::commit` using flush helpers from phase 01.

**In scope:**

- `NodeDefRegistry::commit(&mut self, fs: &mut dyn LpFs, frame, ctx) -> Result<SyncResult, CommitError>`
- Fs writes: create/modify via `write_file_mut`, delete via fs delete API
- `acquire_file_artifact` for paths not in `artifact_path_to_id`
- `store.apply_fs_changes` with `ChangeType::Create` / `Modify` / `Delete`
- Re-derive: `sync_def_artifact` for touched `.toml` artifacts;
  `sync_source_path` for affected asset paths
- `snapshot_def_states` + `build_change_details` → `SyncResult`
- `overlay.clear()` on success only
- Empty overlay → `Ok(SyncResult::default())`

**Out of scope:** integration tests (phase 03+), `RegistryChange::ChangeSet`.

## Implementation details

Follow `commit-contract.md` ordering: resolve → fs write → store bump → re-derive.

Key symbols in `node_def_registry.rs`:

- `sync_def_artifact`, `sync_source_path`, `acquire_file_artifact`
- `classify_changed_path` (reuse or mirror for touched path sets)

Prefer validate-all-serialize before first fs write to reduce partial failure.

## Sub-agent reminders

- Do not commit.
- `sync_fs` behavior unchanged; overlay reads unchanged (already effective_read).

## Validate

```bash
cargo check -p lpc-node-registry
cargo test -p lpc-node-registry
```

Integration tests may not exist yet; existing tests must stay green.
