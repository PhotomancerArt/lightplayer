# M8 Future Work

## Effective SyncResult on apply-only

- **Idea:** `SyncOp::Append` also diffs effective parse state and returns partial `SyncResult` for live engine preview before commit.
- **Why not now:** Engine cutover can use `NodeDefView` for preview; doubles diff logic.
- **Useful context:** `engine-policy-v1.md`; parent M6 policy decision.

## Per-ArtifactEdit log entries

- **Idea:** Finer session log than whole `EditBatch` append — enables single-op undo.
- **Why not now:** v1 batches match client wire shape; coarser log is enough for `since(version)`.
- **Revisit when:** Undo/redo UI needs op-level granularity.

## CRDT / multi-writer session merge

- **Idea:** Concurrent Append from multiple clients with merge.
- **Why not now:** v1 single-writer server + optimistic `SessionVersion` base check.
- **Useful context:** `future.md` on ChangeSet roadmap.

## Session history across commits

- **Idea:** Keep committed session log for audit/undo stack instead of clear-on-commit.
- **Why not now:** Simpler client semantics — commit resets draft baseline.
- **Revisit when:** Persistent undo or collaborative editing.
