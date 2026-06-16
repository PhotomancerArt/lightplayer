# M1 Summary — `lpc-node-registry` + ArtifactStore

### What was built

- Added `lp-core/lpc-node-registry` crate (`no_std` + alloc) with stub modules for M2–M5.
- Removed defunct `lp-core/lpc-slot-mockup` from workspace.
- Implemented requester-owned `ArtifactStore`: acquire/release refcount, `revision`, `apply_fs_changes`.
- Added structured read outcomes: `ArtifactReadState` + `ArtifactReadFailure` (`Deleted`, `NotFound`, `Io`, `InvalidPath`).
- Transient `read_bytes` via `LpFs` (no cached payload on entries).
- 11 unit tests covering acquire, release, fs invalidation, and read paths.

### Decisions for future reference

#### Requester-owned artifacts (not filesystem-registered)

- **Decision:** Entries exist only after `acquire`; fs changes invalidate held paths; `release` at refcount 0 removes entry.
- **Why:** Client/registry drives identity; fs is an invalidation source, not registration.
- **Rejected alternatives:** Auto-register all project files on fs events; no refcount.
- **Revisit when:** Unlikely for this boundary.

#### Structured `ArtifactReadFailure` vs string errors

- **Decision:** `Failed(Deleted | NotFound | Io | InvalidPath)` distinct from `ArtifactError::Resolution` at acquire.
- **Why:** Registry/engine need typed outcomes; `FsError` mapping preserved.
- **Rejected alternatives:** Single `ReadError { message }` string.
- **Revisit when:** M5 overlay may add non-fs read paths (AssetView).

#### Field name `revision` not `content_frame`

- **Decision:** `ArtifactEntry.revision` uses `lpc_model::Revision`.
- **Why:** Content-generation marker; aligns with sync vocabulary, distinct from engine's legacy name.
- **Revisit when:** M6 cutover may map engine tick context naming.

Plan: docs/roadmaps/2026-05-21-artifact-routed-file-reload/m1-artifact-store/
