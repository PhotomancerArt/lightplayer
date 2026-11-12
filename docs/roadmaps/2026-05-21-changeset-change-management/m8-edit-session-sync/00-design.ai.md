# M8 Design — Edit Session + Unified Sync

## Scope

Rename edit vocabulary, introduce versioned **SessionLog**, materialize **SlotOverlay**
from the log, route all registry ingress through **`sync(&[SyncOp])`**, return
**`SyncOutcome`**, and rename **`FsChange` → `FsEvent`**.

**Out of scope:** `lpc-engine`, wire protocol, CRDT, effective `SyncResult` on
apply-only.

## File structure

```
lp-base/lpfs/src/
├── fs_event.rs                     # UPDATE: FsEvent, FsEventKind (was FsChange, ChangeType)
├── lp_fs.rs                        # UPDATE: get_events_since naming (alias ok)
└── impls/                          # UPDATE: lp_fs_mem, lp_fs_std, lp_fs_view, …

lp-app/lpa-server/src/
└── server.rs                       # UPDATE: FsEvent

lp-cli/src/commands/dev/
├── watcher.rs                      # UPDATE: FsEvent
├── sync.rs                         # UPDATE
└── fs_loop.rs                      # UPDATE

lp-core/lpc-node-registry/src/
├── lib.rs                          # UPDATE: re-exports
├── edit/                           # RENAME from change/
│   ├── mod.rs
│   ├── edit_op.rs                  # RENAME artifact_op.rs
│   ├── artifact_edit.rs            # RENAME artifact_change.rs
│   ├── edit_batch.rs               # RENAME change_set.rs
│   ├── edit_target.rs              # RENAME artifact_target.rs
│   ├── edit_error.rs               # RENAME change_error.rs
│   ├── apply.rs                    # UPDATE: Edit* types
│   ├── slot_overlay.rs             # RENAME overlay.rs (SlotOverlay)
│   ├── slot_overlay_entry.rs       # SPLIT if needed
│   ├── def_draft.rs                # RENAME slot_draft.rs
│   └── commit_error.rs             # keep (or sync_error.rs later)
├── registry/
│   ├── sync_op.rs                  # NEW: SyncOp (was registry_change.rs)
│   ├── sync_outcome.rs             # NEW: SyncOutcome
│   ├── session/
│   │   mod.rs
│   │   session_version.rs          # NEW
│   │   session_event.rs            # NEW: Append, Remove, Commit marker, …
│   │   session_log.rs              # NEW: append + since(version)
│   │   session_delta.rs            # NEW
│   │   session_entry_id.rs         # NEW
│   ├── node_def_registry.rs        # UPDATE: sync applies SyncOp batch
│   ├── commit.rs                   # UPDATE: invoked from SyncOp::Commit
│   ├── slot_apply.rs               # UPDATE: Edit* types
│   └── effective_read.rs           # UPDATE: SlotOverlay
├── diff/                           # UPDATE: returns EditBatch
└── tests/                          # UPDATE all integration tests

docs/roadmaps/2026-05-21-changeset-change-management/
├── edit-language.md                # RENAME from change-language.ai.md
├── decisions.ai.md                    # UPDATE: vocabulary + session decisions
├── summary.ai.md                      # UPDATE: M8 gate for parent M6
└── m8-edit-session-sync/             # this plan
```

## Architecture

```text
LAYER 0 — Committed disk notifications
  FsVersion  →  get_events_since  →  FsEvent

LAYER 1 — Edit vocabulary (serde / diff / wire)
  EditBatch { EditBatchId, edits: Vec<ArtifactEdit { EditTarget, ops: [EditOp] }> }

LAYER 2 — Session + materialized pending
  SessionLog (append-only, SessionVersion)
       │ fold
       ▼
  SlotOverlay (path → SlotOverlayEntry: Bytes | DefDraft | Deleted)

LAYER 3 — Unified ingress
  sync(fs, &[SyncOp], frame, ctx) → SyncOutcome

  SyncOp:
    Fs(FsEvent)
    Append { base: SessionVersion, batch: EditBatch }
    Remove { base, entry_ids }
    Commit { base }
    Discard { base, scope }

LAYER 4 — Outcomes
  SyncOutcome {
    session: SessionDelta,           // for clients since last SessionVersion
    committed: SyncResult,           // for engine (fs + commit legs)
    session_version: SessionVersion,
  }

READS
  registry.get()        → committed entries
  NodeDefView.get()     → SlotOverlay ∪ committed (effective)
```

## Main components

### SessionLog

- Monotonic `SessionVersion` (starts 0; increments on each meta-op).
- Append stores `(SessionEntryId, SessionEvent::Append(EditBatch))`.
- `session_since(v) -> SessionDelta` for client pull.
- `Append` / `Remove` / `Discard` require `base == current_version` (optimistic lock).
- **`Commit`**: run existing commit promotion, **clear log**, bump version (fresh draft).

### SlotOverlay

- Derived from SessionLog (rebuild or incremental — implementation choice in phase 4).
- Same semantics as today's `ChangeOverlay`; rename only in phase 2 unless log rebuild forces refactor.

### sync()

Process `SyncOp` batch in order:

1. **Fs** — existing `sync` fs path → merge into `SyncResult.committed`
2. **Append** — validate base, append log, update SlotOverlay
3. **Remove** — tombstone log entries, rebuild overlay
4. **Discard** — clear log entries (scoped), rebuild overlay
5. **Commit** — flush overlay → fs → re-derive → `SyncResult.committed`, clear session

Return combined `SyncOutcome`.

### Thin wrappers (compat)

```rust
pub fn apply_edit_batch(...) -> Result<SyncOutcome, EditError> {
    sync(fs, &[SyncOp::Append { base: session_version(), batch }], ...)
}

pub fn commit(...) -> Result<SyncOutcome, CommitError> {
    sync(fs, &[SyncOp::Commit { base: session_version() }], ...)
}
```

### FsEvent rename

- `FsChange` → `FsEvent`; `ChangeType` → `FsEventKind`.
- `get_changes_since` may alias to `get_events_since` or rename with deprecated alias.

## Validation

```bash
cargo test -p lpc-node-registry
cargo test -p lpfs
cargo test -p lpa-server --no-run
cargo clippy -p lpc-node-registry --all-targets --no-deps -- -D warnings
cargo check -p lpc-node-registry --no-default-features
just check  # before plan commit
```

## Test scenarios (new / updated)

| Test | Behavior |
|------|----------|
| Session append | two Append ops; `session_since` returns both; SlotOverlay reflects last |
| Stale base | Append with wrong `SessionVersion` → error |
| Sync batch | `sync([Append, Commit])` → committed SyncResult + empty session |
| Fs + commit | `sync([Fs(modify glsl), Commit])` in one batch |
| Diff roundtrip | `diff` → EditBatch → sync Append + Commit → equivalent |

## Non-goals

- Engine interpreting `SyncOutcome` (parent M6)
- Wire message types in `lpc-wire`
- Per-`EditOp` log entries (v1 = per `EditBatch` append)
