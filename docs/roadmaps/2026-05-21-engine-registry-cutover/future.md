## Slot provenance / ExplainSlot

- **Idea:** Post-cutover client probe for effective slot resolution (artifact-routed M10).
- **Why not now:** Diagnostic feature; cutover does not require it.
- **Useful context:** `lpc-wire` probe types; engine effective read path after M5.

## ChangeSet replay stress harness

- **Idea:** Replay `EditBatch` streams through full engine on host/emu/device.
- **Why not now:** Needs M4–M5 engine on registry path.
- **Useful context:** ChangeSet `diff` + `assert_equivalent`.

## Server-side session log

- **Idea:** Versioned append log for multi-tab / undo.
- **Why not now:** ChangeSet M8 explicitly dropped session log.
- **Revisit when:** Client needs pull-based pending sync.

## CRDT / multi-writer edits

- **Idea:** Concurrent edit merge on shared projects.
- **Why not now:** Single-writer server v1.
- **Useful context:** ChangeSet roadmap `future.md`.
