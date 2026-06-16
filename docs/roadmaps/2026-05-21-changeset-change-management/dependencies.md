# External Dependencies

This roadmap depends on the **artifact-routed file reload** parallel stack
(M1–M4). Do not start M1 here until those are complete and passing.

## Required (parent roadmap)

| Milestone | Roadmap | Key APIs |
|-----------|---------|----------|
| M1 | [ArtifactStore](../2026-05-21-artifact-routed-file-reload/m1-artifact-store.md) | `acquire_location`, `apply_fs_changes`, `read_bytes` |
| M2 | [NodeDefRegistry](../2026-05-21-artifact-routed-file-reload/m2-node-def-registry.md) | `load_root`, `NodeDefId`, `DefSource`, `NodeDefUpdates` |
| M3 | [SourceFileSlot](../2026-05-21-artifact-routed-file-reload/m3-source-file-slot.md) | `resolve_source_file`, `materialize_source` |
| M4 | [Fs-change harness](../2026-05-21-artifact-routed-file-reload/m4-fs-change-semantics-harness.md) | `sync` → `SyncResult`, `RegistryChange::Fs` |

Validation baseline:

```bash
cargo test -p lpc-node-registry
cargo test -p lpc-node-registry --test fs_change_semantics
```

## Downstream (blocks)

| Milestone | Roadmap | Requires from here |
|-----------|---------|-------------------|
| M6 | [Engine cutover](../2026-05-21-artifact-routed-file-reload/m6-engine-cutover.md) | M6 diff + equivalence gate green |
| M7+ | Server / graph / cleanup | M6 |
