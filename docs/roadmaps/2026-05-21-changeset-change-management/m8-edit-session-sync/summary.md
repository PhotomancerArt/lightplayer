# M8 Summary — Unified Sync + Pending Edit Map

**Status:** complete

## Delivered

- **Pending map** — `SlotOverlay` holds current uncommitted edits keyed by path (no history)
- **Unified sync** — `SyncOp` batch: `Fs`, `Apply`, `Remove`, `ClearPending`, `Commit`
- **CRUD** — `apply_artifact_edit`, `remove_pending_edit`, `discard_slot_overlay`, `apply_edit_batch`
- **Outcomes** — `SyncOutcome { committed, pending_changed }`
- **FsEvent rename** — `FsEvent` / `FsEventKind` in `lpfs` (deprecated aliases kept)
- **Legacy** — `RegistryChange` = `SyncOp` (deprecated)

## Explicitly not on device

- Session log, version cursor, incremental pull, undo history — client-side if ever needed

## Tests

- `tests/pending_sync.rs` — apply, remove, apply+commit, fs+commit batch
- Existing overlay/commit/fs tests unchanged in behavior

## Validation

```bash
cargo test -p lpc-node-registry
cargo clippy -p lpc-node-registry --all-targets --no-deps -- -D warnings
cargo check -p lpc-node-registry --no-default-features
```

## Gate

Parent **M6 engine cutover** may wire `sync(&[SyncOp])` when ready.
