# Milestone 8: Edit Vocabulary, Session Log, and Unified Sync

## Title And Goal

Rename the edit vocabulary (`Edit*` / `SlotOverlay`), add a **versioned session
log** for pending client edits, and unify registry ingress through **`sync(&[SyncOp])`**
returning **`SyncOutcome`**. Align filesystem notifications with **`FsEvent`**.

**Gates parent artifact-routed M6 engine cutover** — engine and server need a
single sync boundary and client-visible pending state before cutover is meaningful.

## Parallel Build

`lpc-node-registry` + `lpfs` + server/cli callers only. **No `lpc-engine` edits**
in this milestone.

## Suggested Plan Location

[`m8-edit-session-sync/`](m8-edit-session-sync/)

## Scope

In scope:

- Layer 1 renames: `change/` → `edit/`, `EditOp`, `ArtifactEdit`, `EditBatch`, …
- Layer 2: `SlotOverlay`, `SessionLog`, `SessionVersion`, `SessionDelta`
- Layer 3: `SyncOp`, extended `sync()`, `SyncOutcome`
- Layer 0: `FsChange` → `FsEvent` in `lpfs` and direct callers
- Harness tests updated; session + sync integration tests
- Roadmap docs + `decisions.md` vocabulary section

Out of scope:

- `lpc-engine` cutover (parent M6)
- Full `lpc-wire` protocol messages
- CRDT / multi-writer merge
- Effective-state `SyncResult` on apply-only (defer; document in `future.md`)

## Dependencies

- ChangeSet M1–M7 complete (overlay, commit, diff gate)

## Execution Strategy

Phased: vocabulary rename → overlay rename → FsEvent → session log → SyncOp.
Single commit at plan end unless a rename checkpoint is needed mid-plan.

Suggested chat opener:

> M8 plan: Edit* rename + SessionLog + SyncOp unified sync. Review phases?
