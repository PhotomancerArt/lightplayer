# Phase 01 — Unified sync API + SyncResult

**Dispatch:** [sub-agent: yes, model: composer-2.5-fast, parallel: -]

## Scope of phase

Consolidate M2 two-step flow into **`NodeDefRegistry::sync(changes) -> SyncResult`**.

**In scope:**

- Registry **owns** `ArtifactStore` (move field inside `NodeDefRegistry`)
- `registry_change.rs` — `RegistryChange::Fs(FsChange)` 
- `sync_result.rs` — `SyncResult { def_updates, source_revisions, change_details }`
  (source/details filled in later phases; empty vecs OK initially)
- New `sync(fs, changes, frame, ctx) -> SyncResult`:
  - apply fs changes to internal store
  - re-derive affected defs (by changed paths)
  - diff → `NodeDefUpdates` in result
- `NodeDefUpdates` → **`Vec<NodeDefId>`** per field
- Update `load_root` signature: no external `store` param
- Migrate existing M2 unit tests to new API
- `harness/fixtures.rs` — TOML constants for scenarios

**Out of scope:** Source revision bumps, DefChangeDetail, integration test file.

## API migration

```rust
// Before
store.apply_fs_changes(&changes, frame);
let updates = registry.sync(&mut store, fs, frame, ctx);

// After
let result = registry.sync(fs, &changes.iter().map(RegistryChange::Fs).collect(), frame, ctx);
let updates = result.def_updates;
```

For M4 ergonomics, also accept:

```rust
pub fn sync_fs(&mut self, fs, changes: &[FsChange], frame, ctx) -> SyncResult
```

as thin wrapper over `sync` with `RegistryChange::Fs` mapping.

## Internal simplification

- Drive re-derive from **paths in `changes`**, not `artifact_last_revision` map.
- Keep `entries` / `source_index` maps until DenseIdMap pass (optional).

## Memory

- `NodeDefUpdates`: `Vec` not `BTreeSet`
- `SyncResult`: `Vec` fields

## Sub-agent Reminders

- Do not commit.
- Breaking API change within crate is OK; fix all tests.
- No `set_current_revision` in tests.

## Validate

```bash
cargo test -p lpc-node-registry
cargo check -p lpc-node-registry
```
