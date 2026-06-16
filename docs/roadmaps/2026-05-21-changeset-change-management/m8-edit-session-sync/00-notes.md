# M8 Plan Notes — Edit Session + Unified Sync

## Scope of work

Replace the overloaded **Change\*** vocabulary with four clear layers, and
implement the **session log** that was deferred from M5:

1. **Edit vocabulary** — wire/serde authoring types (`EditOp`, `ArtifactEdit`, `EditBatch`)
2. **Session + SlotOverlay** — versioned pending log; materialized overlay derived from log
3. **Sync ingress** — `sync(&[SyncOp])` replaces split `apply_changeset` / `commit` / fs-only `RegistryChange`
4. **Sync outcomes** — `SyncOutcome { session, committed, session_version }`
5. **FsEvent** — rename `FsChange` → `FsEvent` in `lpfs` and callers

This milestone completes the registry contract the engine and server need before
parent **M6 engine cutover**.

## Current state

### Implemented (M1–M7)

- `lpc-node-registry/src/change/` — `ArtifactOp`, `ArtifactChange`, `ChangeSet`, `ChangeOverlay`
- `NodeDefRegistry::apply_changeset` → overlay; `commit` → fs + `SyncResult`; `sync` → `RegistryChange::Fs` only
- No pending version cursor; no `since(version)` for client edits
- `lpfs::FsChange` + `FsVersion` + `get_changes_since` used by `lpa-server`, `lp-cli`, registry tests
- `diff` feature generates `ChangeSet` (host harness)

### Design gap (from M4/M5)

M4 design intended `RegistryChange` to grow ChangeSet variants on **`sync`**. M5
implemented dedicated `apply_changeset` / `commit` instead (harness shortcut).
Engine policy (`engine-policy-v1.md`) assumes `sync` → `SyncResult` for all
change sources.

### Rename blast radius (approximate)

| Symbol | Files touching (repo-wide grep) |
|--------|----------------------------------|
| ChangeSet / ArtifactChange / ChangeOverlay | ~40+ under `lpc-node-registry`, docs |
| FsChange | `lpfs`, `lpa-server`, `lp-cli`, `fw-esp32`, registry tests |

## Agreed vocabulary (from design discussion)

| Layer | Old | New |
|-------|-----|-----|
| 0 | `FsChange` | `FsEvent` |
| 0 | `ChangeType` | `FsEventKind` (optional) |
| 1 | `ArtifactOp` | `EditOp` |
| 1 | `ArtifactChange` | `ArtifactEdit` |
| 1 | `ChangeSet` | `EditBatch` |
| 1 | `ChangeSetId` | `EditBatchId` |
| 1 | `ArtifactTarget` | `EditTarget` |
| 1 | `ChangeError` | `EditError` |
| 1 | `change/` module | `edit/` |
| 2 | `ChangeOverlay` | `SlotOverlay` |
| 2 | `OverlayEntry` | `SlotOverlayEntry` |
| 2 | `SlotDraft` | `DefDraft` |
| 2 | (new) | `SessionVersion`, `SessionEvent`, `SessionLog`, `SessionDelta` |
| 3 | `RegistryChange` | `SyncOp` |
| 4 | (new) | `SyncOutcome` |
| 4 | `SyncResult` | keep (committed effects) |

## Open questions

| # | Question | Context | Suggested answer |
|---|----------|---------|------------------|
| Q1 | Gate parent M6 on M8 (not just M6 diff)? | Parent cutover needs SyncOp contract | **Yes** — update parent M6 deps |
| Q2 | `SlotDraft` → `DefDraft`? | Pairs with SlotOverlay | **Yes** |
| Q3 | `FsEvent` rename in same milestone? | Touches lpfs, server, cli | **Yes**, dedicated phase after registry renames |
| Q4 | Temporary type aliases (`type ChangeSet = EditBatch`)? | Eases doc/test migration | **Yes**, one release; grep CI for deprecated uses |
| Q5 | Session log granularity | Append whole `EditBatch` vs per `ArtifactEdit` | **Append EditBatch** for v1; log entry gets stable `SessionEntryId` |
| Q6 | After `SyncOp::Commit`, reset session log? | Client `since(version)` semantics | **Clear log, bump SessionVersion** (fresh draft baseline) |
| Q7 | Keep `apply_edit_batch` / `commit` as wrappers? | Ergonomics for tests | **Yes**, thin delegates to `sync([...])` |
| Q8 | `SyncOp::Append` requires matching `SessionVersion`? | Optimistic concurrency | **Yes** — return `EditError::StaleSession { expected, actual }` |
| Q9 | Apply-only returns committed `SyncResult`? | Live preview before commit | **No in M8** — `SyncOutcome.committed` empty on Append-only; engine uses `NodeDefView` for preview until parent M6 policy |
| Q10 | Rename roadmap title "ChangeSet" → "Edit session"? | Docs only | **Defer** — update `change-language.md` → `edit-language.md`, keep roadmap folder name |

## Notes

- User prefers **Edit\*** over Author\* for layer 1.
- **SlotOverlay** holds Bytes + DefDraft + Deleted (assets included); name accepted with that caveat.
- M7 doc pass (uncommitted) cleaned milestone comments; M8 will update vocabulary in docs.
- Single commit at plan end per `/plan` process unless rename checkpoint needed.

## Answers

*(Fill as user confirms Q1–Q10.)*

### Nomenclature phase (started)

- **Done:** Layer 1 + Layer 2 renames in `lpc-node-registry` (see [`vocabulary.md`](vocabulary.md)).
- **Pending:** Session log, `SyncOp`, `FsEvent`.
