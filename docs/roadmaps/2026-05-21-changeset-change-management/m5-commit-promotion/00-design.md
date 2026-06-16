# M5 Design — Commit Promotion

## Scope

Promote `ChangeOverlay` to committed state: fs write → store revision bump →
re-derive `entries` → `SyncResult` → clear overlay. Prove **D2**, **D5**, and
C2 post-commit `NodeDefUpdates` shape.

**`lpc-engine` untouched.**

Depends on M1–M4 (overlay, effective reads, file ops, slot ops + serialize).

## File structure

```
lp-core/lpc-node-registry/src/
├── change/
│   ├── commit_error.rs          # NEW — CommitError
│   └── overlay.rs               # add path iteration helper if needed
├── registry/
│   ├── commit.rs                # NEW — flush + commit impl (#[path] from node_def_registry)
│   ├── node_def_registry.rs     # pub fn commit(...)
│   ├── slot_apply.rs            # reuse serialize_slot_draft
│   └── sync_result.rs           # unchanged shape
└── tests/
    └── commit_promotion.rs      # NEW — D2, D5, C2 post-commit
```

```
docs/roadmaps/.../m5-commit-promotion/
├── commit-contract.md           # behavioral contract (this milestone)
├── 00-notes.md
├── 00-design.md
└── 01–06 phase files
```

## Architecture

```text
Client                          NodeDefRegistry
  │                                    │
  ├─ apply_changeset ───────────────► ChangeOverlay (pending)
  ├─ view().get() ──────────────────► effective_read (overlay ∪ base)
  │
  └─ commit(fs, frame, ctx) ────────► commit.rs
                                         │
                    ┌────────────────────┴────────────────────┐
                    │ 1. Early exit if overlay empty          │
                    │ 2. Resolve each path → bytes/action     │
                    │    SlotDraft → serialize_slot_draft     │
                    │    Bytes → raw                          │
                    │    Deleted → fs delete                  │
                    │ 3. Write LpFs (create/modify/delete)    │
                    │ 4. acquire_file_artifact (new paths)    │
                    │ 5. store.apply_fs_changes (bump rev)    │
                    │ 6. snapshot_def_states (before)         │
                    │ 7. sync_def_artifact (each .toml)       │
                    │ 8. sync_source_path (asset deps)        │
                    │ 9. reconcile_artifact_refs              │
                    │10. build_change_details → SyncResult    │
                    │11. overlay.clear()                      │
                    └─────────────────────────────────────────┘
                                         │
              get() / entries ◄──────────┘ committed cache
```

### API

```rust
impl NodeDefRegistry {
    /// Promote all pending overlay entries to committed store + entries.
    /// Returns factual SyncResult. Clears overlay on success.
    pub fn commit(
        &mut self,
        fs: &mut dyn LpFs,
        frame: Revision,
        ctx: &ParseCtx<'_>,
    ) -> Result<SyncResult, CommitError>;
}
```

**Unchanged:** `sync` / `sync_fs` remain fs-reload only. `discard_overlay` unchanged.

### Overlay → bytes resolution

| `OverlayEntry` | Fs action | Store |
|----------------|-----------|-------|
| `Bytes(v)` | write `v` | acquire/bump via fs change |
| `SlotDraft(d)` | write `serialize_slot_draft(&d.def, ctx)?` | same |
| `Deleted` | delete file | `ChangeType::Delete` bump |

### Re-derive strategy

Reuse existing helpers from `node_def_registry.rs`:

- `sync_def_artifact` for each touched `.toml` artifact id
- `sync_source_path` for touched asset paths referenced by loaded defs
- `build_change_details` + `snapshot_def_states` for `SyncResult`

Classify touched paths:

- `.toml` → def artifact sync set
- other → source path sync set (if path appears in `source_path_index` or affects materialized deps after def sync)

For overlay paths not yet in `artifact_path_to_id`: `acquire_file_artifact` after fs write.

### Failure semantics

- Validate serialize + fs writes in an order that allows rollback where practical.
- If re-derive fails after fs write: document behavior in `commit-contract.md`;
  prefer **fail before mutating `entries`** when validation catches errors early.
- On `CommitError`: **`entries` unchanged**, **overlay retained**.
- Empty overlay: return `Ok(SyncResult::default())`.

### D5 precedence

While overlay active on path `P`:

- `read_effective_bytes`, `view().get`, `materialize_source` → overlay (already M2–M4)
- `sync_fs` on `P` → bumps store revision but **does not** replace overlay reads
- After successful `commit` → overlay cleared; `sync_fs` on `P` follows fs/store rules

## Tests

| Test | Story |
|------|-------|
| `d2_commit_updates_committed_and_clears_overlay` | SetSlot on clock; commit; `get()` matches view; overlay empty |
| `d2_commit_slot_draft_serializes_to_fs` | After commit, fs file contains serialized TOML |
| `d5_overlay_wins_over_fs_until_commit` | overlay + fs diverge; view=overlay; sync_fs doesn't clobber view |
| `d5_post_commit_fs_sync_applies` | after commit, fs change updates committed |
| `c2_inline_child_in_sync_result_after_commit` | playlist inline patch; child in `def_updates.changed`, not root |

Fixtures: `load_clock`, `load_playlist_with_inline_child`, `load_shader_project`.

## Non-goals

- `RegistryChange::ChangeSet` batch type
- Compose-from-blank / `load_root` without pre-existing project (M6)
- Engine cutover
- Writing committed bytes without fs (embedded store-only path)

## Validation

```bash
cargo test -p lpc-node-registry
cargo test -p lpc-node-registry --test commit_promotion
```
