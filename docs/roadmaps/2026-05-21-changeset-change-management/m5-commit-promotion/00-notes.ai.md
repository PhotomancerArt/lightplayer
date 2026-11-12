# M5 Notes — Commit Promotion

## Scope

Implement **commit**: promote overlay → durable bytes (fs write in harness) →
`ArtifactStore` revision bump → re-derive `entries` → **`SyncResult`** → clear
overlay. Prove **D2** and **D5**.

**Out of scope:** engine cutover (parent M6), diff/equivalence (M6 here),
`RegistryChange::ChangeSet` wire integration (optional; plan decides), compose-from-blank
(A1 — M6 gate).

## Plan directory gap (M3, M4)

M1 and M2 have full plan folders (`m1-change-language-overlay/`,
`m2-effective-projection/`) with `00-notes.ai.md`, `00-design.ai.md`, and numbered
phases. **M3 and M4 were implemented without plan directories** — only milestone
overview stubs exist:

- `m3-asset-overlay.ai.md` (suggested `m3-asset-overlay/` never created)
- `m4-node-slot-patches.ai.md` (suggested `m4-node-slot-patches/` never created)

Implementation landed via direct execution (commits `81ff051b` M3, `9234bb94` M4).
Tests live under `lp-core/lpc-node-registry/tests/` (`asset_overlay.rs`,
`slot_overlay.rs`, etc.) rather than a unified `tests/changeset/` tree mentioned
in overview docs.

**Optional follow-up (not M5):** backfill lightweight `summary.ai.md` in
`m3-asset-overlay/` and `m4-node-slot-patches/` documenting what shipped and
where tests live. Not blocking M5.

## Current codebase (post-M4)

```
lp-core/lpc-node-registry/src/
├── change/
│   ├── overlay.rs              # Bytes | SlotDraft | Deleted
│   ├── apply.rs                # SetBytes/Delete only (slot ops via registry)
│   └── slot_draft.rs
├── registry/
│   ├── node_def_registry.rs    # apply_change/changeset, discard; NO commit
│   ├── effective_read.rs       # overlay ∪ store reads for preview
│   ├── slot_apply.rs           # slot op apply + serialize_slot_draft
│   ├── sync_result.rs          # SyncResult shape (reuse on commit)
│   └── registry_change.rs      # Fs only
└── artifact/artifact_store.rs  # freshness cache; read_bytes from fs; no write API

Tests (changeset-related):
├── overlay_lifecycle.rs        # D1, D3, implicit create
├── effective_projection.rs     # view vs committed
├── asset_overlay.rs            # C4*
├── slot_overlay.rs             # C1, C2
└── fs_change_semantics.rs      # S1–S6 (fs sync path — commit should mirror)
```

### Apply vs commit today

| Step | Status |
|------|--------|
| `apply_change` / `apply_changeset` → overlay | Done |
| `view().get()` effective preview | Done |
| `discard_overlay()` | Done |
| `commit()` → base + SyncResult | **Missing** |
| `read_artifact_state` uses overlay | **No** — reads store/fs only |
| New overlay paths in store | **No** — implicit create is overlay-only until commit |

Commit must bridge: overlay bytes → fs (harness) → store revision →
`sync_def_artifact` / `derive_inventory` (existing fs-sync path).

## Resolved decisions (from roadmap)

- Commit reuses parent M4 re-derive path (`sync_def_artifact`, `derive_inventory`).
- `discard` = overlay clear only; **commit** = only path that mutates committed
  `entries` from client edits.
- All-or-nothing commit; failure leaves base untouched (overlay may retain pending).
- D5: uncommitted overlay wins on effective read; fs bump marks stale but does
  not clobber overlay until commit/discard; on commit, client ChangeSet wins.

## Resolved questions (2026-05-21)

| # | Decision |
|---|----------|
| Q1 | Dedicated `NodeDefRegistry::commit(...) -> Result<SyncResult, CommitError>`; do not overload `sync()` in M5 |
| Q2 | Commit writes overlay content to `LpFs`, then bumps store via existing fs-change pattern |
| Q3 | Flush all overlay paths → fs → acquire/bump store → re-derive → clear overlay on success |
| Q4 | M5 tests assume `load_root` already ran; compose-from-blank deferred to M6 |
| Q5 | Failed commit: base unchanged, overlay retained |
| Q6 | Commit uses overlay entry variant as-is; no merge between `SlotDraft` and `Bytes` |
| Q7 | D5 harness: overlay wins over fs until commit; post-commit fs sync follows committed rules |

## User stories (this milestone)

| ID | Story | How |
|----|-------|-----|
| D2 | Commit → base updated; overlay clear | `get()` matches post-commit; `overlay_active()` false |
| D5 | Overlay vs fs-change precedence | Harness above |
| C2 post-commit | Inline child in `NodeDefUpdates.changed` | Commit slot patch on playlist; child id in SyncResult |

## Validation baseline

```bash
cargo test -p lpc-node-registry
```

Must remain green; add `tests/commit_promotion.rs` or extend `overlay_lifecycle.rs`.
