# ChangeSet Change Management — Summary

## Status

Complete on branch `codex/incremental-artifact-reload` (M1–M6 implemented; M7 doc
and validation pass).

## Delivered

`lpc-node-registry` now supports client-driven edits as ordered
[`ChangeSet`](../../lp-core/lpc-node-registry/src/change/change_set.rs) batches:

```text
apply_changeset → ChangeOverlay
NodeDefView.get() → effective (overlay ∪ committed)
commit(fs)        → write fs → re-derive entries → SyncResult → clear overlay
discard_overlay   → drop pending edits
```

| Milestone | Summary |
|-----------|---------|
| M1 | Change language types, `ChangeOverlay`, apply/discard |
| M2 | Effective projection via `NodeDefView` |
| M3 | Asset `SetBytes`/`Delete`, materialize from overlay |
| M4 | Slot ops, TOML serialize for slot drafts |
| M5 | Commit promotion to filesystem + `SyncResult` |
| M6 | `diff(base, target)` + equivalence gate (`diff` feature) |

Per-milestone notes: `m1-change-language-overlay/summary.md` through
`m6-diff-equivalence-gate/summary.md`.

## Public API

- **Read:** `NodeDefView`, `ParseCtx`, committed `NodeDefRegistry::get`
- **Client edit:** `ChangeSet`, `NodeDefRegistry::apply_changeset`,
  `discard_overlay`, `commit`
- **Filesystem reload:** `sync` / `sync_fs` (unchanged from artifact-routed M4)
- **Harness (`diff` feature):** `ProjectSnapshot`, `diff`, `assert_equivalent`

Embedded consumers should depend with `default-features = false` to omit the diff
harness.

## Validation

```bash
cargo test -p lpc-node-registry
cargo clippy -p lpc-node-registry --all-targets --no-deps -- -D warnings
cargo check -p lpc-node-registry --no-default-features
```

73 integration + unit tests at last count.

## Unblocks

Parent [artifact-routed M6 engine cutover](../2026-05-21-artifact-routed-file-reload/m6-engine-cutover.md)
— ChangeSet M6 diff + equivalence gate is green.

## Out of scope (see `future.md`)

- Wire ChangeSet protocol to clients
- `RegistryChange::ChangeSet` variant (fs notifications only today)
- Concurrent merge / CRDT
- Parent M7–M10 server, graph, provenance
