# Phase 2: Apply Upsert (Replace DefDraft Path)

## Scope of phase

Switch registry apply path from materialized `DefDraft` to **`ArtifactOverlay` upsert**.
Replace `NodeDefRegistry` field `overlay: SlotOverlay` with `overlay: ArtifactOverlay`.

**In scope:**

- `registry/node_def_registry.rs` — field type, `apply_artifact_edit`, introspection
  methods (`slot_overlay_active` → `overlay_active` or keep name with deprecated alias)
- `registry/slot_apply.rs` — upsert `SlotEdit` into overlay; remove `fork_slot_draft` /
  `apply_def_draft`
- `edit/apply.rs` — asset ops upsert into `ArtifactOverlay` (not `SlotOverlay`)
- Rename internal uses: `slot_overlay` → `overlay` where touched

**Out of scope:**

- Effective read / projection (phase 3) — tests that read effective state may fail until
  phase 3; keep compile green by leaving effective_read temporarily on old paths OR
  stub projection as committed-only with TODO — **prefer minimal stub in effective_read
  that returns committed until phase 3** only if tests block; coordinate in report.
- Commit (phase 4)
- Delete old files (phase 6)

## Code organization reminders

- Apply logic stays in `slot_apply.rs` + `edit/apply.rs`.
- Registry orchestrates; overlay module holds storage only.

## Sub-agent reminders

- Do **not** commit.
- Do **not** expand scope.
- Do **not** suppress warnings or weaken tests.
- If effective tests fail, document which fail and why (expected until phase 3); do not
  delete tests.

## Implementation details

### Slot apply (`slot_apply.rs`)

Replace body of `apply_slot_op`:

```rust
pub(crate) fn apply_slot_op(
    &mut self,
    path: LpPathBuf,
    op: &SlotEdit,
    ...
) -> Result<(), EditError> {
    ensure_toml_path(&path)?;
    let location = self.resolve_location_for_path(&path)?; // or existing helper
    let pending = self.overlay.ensure_pending(location);
    pending.upsert_slot(op.path.clone(), op.clone()); // or upsert with op fields
    Ok(())
}
```

Remove: `fork_slot_draft`, `fork_committed_def` from apply path (keep parse helpers if
projection needs them in phase 3).

**Apply full op:** upsert stores the **`SlotEdit` as sent** (including path inside op).
If incoming batch has multiple ops same path, last wins via sequential upsert.

### Asset apply (`edit/apply.rs`)

Change signature to accept `&mut ArtifactOverlay`:

```rust
pub(crate) fn apply_asset_op(
    overlay: &mut ArtifactOverlay,
    location: ArtifactLocation,
    op: &AssetEdit,
) -> Result<(), EditError>
```

Map:

- `AssetEdit::Delete` → `pending.set_asset(AssetPending::Delete)`
- `AssetEdit::ReplaceBody(text)` → `AssetPending::ReplaceBody(text.into_bytes())`

Registry resolves `EditTarget` → `ArtifactLocation` before calling.

### Registry (`node_def_registry.rs`)

- `overlay: ArtifactOverlay`
- `apply_artifact_edit`: pass location + ops to overlay upsert
- Update: `slot_overlay_active` → delegate to `overlay.is_empty()` negated
- Update: `slot_overlay_contains_path` → resolve path to location, `overlay.contains`
- Update: `slot_overlay_bytes` → return bytes only when `AssetPending::ReplaceBody`
  (not for slot-only pending)
- `remove_pending_edit`: `overlay.remove(&location)`
- `discard_slot_overlay`: `overlay.clear()`

Add private helper if needed:

```rust
fn artifact_location_for_edit(&self, path: &LpPathBuf) -> Result<ArtifactLocation, EditError>
```

### Tests

Update or add unit tests in `slot_apply` / registry for:

- Apply slot op → overlay has one key, no DefDraft
- Apply asset op → overlay has asset pending, slots empty
- Second slot op same path → one key updated

## Validate

```bash
cargo check -p lpc-node-registry
cargo test -p lpc-node-registry overlay_lifecycle
cargo test -p lpc-node-registry pending_sync
```

Note: effective/commit tests may fail — list in report if so.
