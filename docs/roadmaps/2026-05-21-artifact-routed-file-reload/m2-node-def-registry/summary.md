# M2 Summary — NodeDefRegistry + NodeDefUpdates

## What was built

- **`NodeDefRegistry`** in `lpc-node-registry` with driver API:
  - `load_root(store, fs, root_path, frame, ctx)` — bootstrap from any root node TOML
  - `sync(store, fs, frame, ctx)` — re-derive after driver `apply_fs_changes`
- Core types: `NodeDefId`, `DefSource`, `NodeDefState`, `NodeDefEntry`, `NodeDefUpdates`,
  `ParseCtx`, `RegistryError`
- **Def walker** for `Project` + `Playlist` invocations (`nodes[key]`, `entries[key].node`)
- **Shell/body diff** — inline child edits do not mark parent `changed`; kind flips do
- **`NodeDefView`** stub (base registry lookup; ChangeSet overlay deferred to M5)
- **22 tests** (`cargo test -p lpc-node-registry`), including gate scenarios T1–T5

## Decisions for future reference

#### Driver-owned store + load_root/sync

- **Decision:** Driver applies `ArtifactStore::apply_fs_changes`; registry exposes
  `load_root` + `sync` only.
- **Why:** Same loop for fs reload (M2/M4) and ChangeSet commit (M5).
- **Rejected alternatives:** Public per-file `register_file`; registry calling
  `apply_fs_changes` internally.

#### Kind change → delete/recreate (M6)

- **Decision:** Registry reports `changed`; engine must delete/recreate runtime
  nodes when bound def kind flips.
- **Why:** Kind determines runtime type and lifecycle.
- **Revisit when:** M6 engine cutover.

Plan: `docs/roadmaps/2026-05-21-artifact-routed-file-reload/m2-node-def-registry/`
