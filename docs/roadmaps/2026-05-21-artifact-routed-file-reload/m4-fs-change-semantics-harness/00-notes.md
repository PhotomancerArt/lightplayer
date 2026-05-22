# M4 Plan Notes — Fs-Change Semantics Harness

## Scope

Refactor registry to **simple stateful sync**: apply changes → update state →
return **`SyncResult`**. Prove fs-change semantics in tests. No `lpc-engine` edits.

## API direction (user-aligned)

**Before (M2):** caller owns `ArtifactStore`, calls `apply_fs_changes`, then
`registry.sync()` with no changes parameter.

**After (M4):** registry owns state; one call:

```rust
let result = registry.sync(fs, changes, frame, ctx);
// result.def_updates, result.source_revisions, result.change_details
```

- Functional: state in, changes in, summary out.
- No external tracking layers or harness-side diff indexes.
- M5 extends `RegistryChange` with ChangeSet ops — same `sync`.

Engine interprets `SyncResult`; registry does not emit `RefreshNode` etc.

## What `sync` does internally

1. Snapshot before (def state + source versions) — **inside** sync only.
2. Apply `RegistryChange` batch to artifact store.
3. Re-derive defs for affected artifacts.
4. Diff → `SyncResult`.

Remove or fold **`artifact_last_revision`** — change batch drives what to
re-derive (plus source-dependency lookup for file-only edits like GLSL).

## Source file bumps (S2/S3)

Not a separate harness index. Registry records source dependencies on entries
(at load / re-derive). When sync applies a change to `/shader.glsl`, registry
finds dependent defs, re-materializes, emits `source_revisions` if version bumped.

Uses internal `source_bridge` (production `ShaderDef` / fixture `SvgPath` → M3).

## Memory

- `SyncResult` and `NodeDefUpdates` use **`Vec`**, not `BTreeSet`/`BTreeMap`.
- Cold path: RAM over CPU; `lp-collection` when it helps (`DenseIdMap` for entries).

## Open questions — resolved

| Q | Resolution |
|---|------------|
| ReloadReport name | **`SyncResult`** — nothing "reloads" at registry layer |
| Two-step apply + sync | **Single `sync(changes)`** |
| Harness source index | **Inside registry** as entry deps, not harness module |
| Engine actions | **`engine-policy-v1.md` only** |
| Public ArtifactStore | **Owned by registry**; not caller-facing in sync path |

## Out of scope

- `lpc-engine` cutover (M6)
- ChangeSet variant on `RegistryChange` (M5 — enum stub OK in M4)
- Registry internal map rewrite to `DenseIdMap` (optional stretch)

## Dependencies

M1–M3 complete. M4 refactors M2 public API (breaking for tests; same crate).
