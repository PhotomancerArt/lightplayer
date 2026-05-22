# M5 Summary — Commit Promotion

## Status

Implemented on branch `codex/incremental-artifact-reload` (uncommitted at handoff).

## Delivered

### lpc-node-registry

- `change/commit_error.rs` — `CommitError` (Fs / Serialize / Registry)
- `change/overlay.rs` — `iter_entries()` for commit flush
- `registry/commit.rs` — `commit_overlay`: flush overlay → fs → store bump → re-derive → clear overlay
- `NodeDefRegistry::commit()` — public entry point returning `SyncResult`
- `restore_entry_states()` — rollback `entries` on failed commit; overlay retained

### Flow

```
apply_changeset → ChangeOverlay
view().get()    → effective (overlay ∪ base)
commit(fs)      → write fs → apply_fs_changes → sync_def_artifact/sync_source_path → SyncResult → clear overlay
sync_fs()       → fs-reload only (unchanged)
```

## Tests

`lp-core/lpc-node-registry/tests/commit_promotion.rs`:

- D2 — commit updates `get()`, clears overlay, fs has serialized TOML
- D2 — SetBytes commit path
- D5 — overlay wins over stale fs until commit
- D5 — `sync_fs` does not clobber overlay view
- D5 — post-commit `sync_fs` updates committed state
- C2 — inline child in `SyncResult.def_updates.changed` after commit
- empty overlay commit is no-op

Unit: `OverlayCommitPlan` slot-draft serialization in `registry/commit.rs`.

## Validation

```bash
cargo test -p lpc-node-registry
cargo test -p lpc-node-registry --test commit_promotion
cargo clippy -p lpc-node-registry --all-targets --no-deps -- -D warnings
```

68 integration tests pass (60 pre-M5 + 8 commit).

## Known limits (M6+)

- Compose-from-blank (A1) not yet proven — requires M6 diff gate
- New overlay `.toml` paths fork `NodeDef::default()` (Project) until kind SetSlot
- `MapInsert` / `MapRemove` / `OptionSet` not integration-tested
- `RegistryChange` still `Fs` only — no `ChangeSet` variant
- Failed commit may leave fs partially written (documented in `commit-contract.md`)

## Next

M6 — compose project from changes alone (A1 blank→basic proof).
