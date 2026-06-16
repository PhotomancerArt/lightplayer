# Phase 4: Commit — Fold Pending → Filesystem

## Scope of phase

Rewrite commit promotion to **fold pending map** into filesystem writes instead of
serializing `DefDraft`.

**In scope:**

- `registry/commit.rs`
- Integration with existing `sync_def_artifact` after fs write

**Out of scope:**

- Public pending introspection API (phase 5)
- Wire

## Code organization reminders

- Commit plan struct builds writes from `ArtifactOverlay::iter()`, not `SlotOverlayEntry`.
- Reuse `serialize_slot_draft` / projection for slot-only pending.

## Sub-agent reminders

- Do **not** commit.
- Do **not** expand scope.
- All commit_promotion tests must pass.

## Implementation details

### Commit plan from overlay

Replace `SlotOverlayCommitPlan::from_slot_overlay`:

```rust
struct OverlayCommitPlan {
    writes: Vec<(LpPathBuf, Vec<u8>)>,
    deletes: Vec<LpPathBuf>,
}

impl OverlayCommitPlan {
    fn from_overlay(
        overlay: &ArtifactOverlay,
        store: &ArtifactStore,
        fs: &dyn LpFs,
        ctx: &ParseCtx<'_>,
    ) -> Result<Self, CommitError>;
}
```

For each `(location, pending)` in `overlay.iter()`:

1. Resolve `LpPathBuf` from `location.file_path()`
2. **Asset pending:**
   - `Delete` → push to `deletes`
   - `ReplaceBody(bytes)` → push to `writes`
3. **Slot pending only** (asset None):
   - Read committed bytes from store/fs
   - Project with `project_artifact_bytes` / def fold
   - Serialize TOML → push to `writes`
4. Skip empty pending buckets

Do **not** write if projected bytes equal committed (optional optimization — OK to skip
for v1).

### `commit_slot_overlay` flow

Keep existing order:

1. Build plan from overlay
2. Write/delete fs
3. `store.apply_fs_changes`
4. Register new paths
5. `sync_def_artifact` for affected `.toml` locations
6. `reconcile_artifacts`
7. `overlay.clear()`

Remove all `SlotOverlayEntry` / `DefDraft` / `serialize_slot_draft(draft.def)` paths
from commit.

### Edge cases

- Implicit create: path in overlay but not in store — register on commit (existing behavior)
- Slot + asset mutual exclusion already enforced at apply — commit sees one or the other
- Empty overlay → early return (existing)

## Validate

```bash
cargo test -p lpc-node-registry commit_promotion
cargo test -p lpc-node-registry pending_sync
cargo test -p lpc-node-registry project_diff
```
