# Phase 02 — Apply Pipeline

**Dispatch:** sub-agent: yes | parallel: 03 | **Depends on:** 01

## Scope of phase

Route apply through `ArtifactEdit::{Slot, Asset}`; eliminate cross-kind
`UnsupportedOp` guards.

**In scope:**

- `edit/apply.rs` — apply `ArtifactEdit::Asset` (and batch loop)
- `registry/slot_apply.rs` — `SlotEdit` only
- `registry/node_def_registry.rs` — `apply_artifact_edit` / `apply_edit_batch`
- `edit/edit_error.rs` — remove `UnsupportedOp` if unused; keep if still needed elsewhere

**Out of scope:** diff, integration test updates (phase 04), docs.

## Code organization reminders

- `apply_asset_op(overlay, path, &AssetEdit)` in `apply.rs` (private).
- Slot apply stays in `slot_apply.rs`.

## Sub-agent reminders

- Do not commit.
- Do not weaken `ensure_toml_path` — slot edits on `.glsl` still error.
- Do not suppress warnings.

## Implementation details

### `apply.rs`

Replace `apply_op(..., &EditOp)` with:

```rust
pub fn apply_artifact_edit(...) {
    let path = resolve_path(...)?;
    match edit {
        ArtifactEdit::Asset { ops, .. } => {
            for op in ops { apply_asset_op(slot_overlay, path.clone(), op)?; }
        }
        ArtifactEdit::Slot { .. } => {
            return Err(EditError::UnsupportedOp { … }); // OR delegate to registry only
        }
    }
}
```

**Decision:** `edit/apply.rs` is the low-level overlay API used without registry
context. Options:

1. **Preferred:** `apply_artifact_edit` in `apply.rs` handles **Asset only**;
   `NodeDefRegistry::apply_artifact_edit` matches full `ArtifactEdit` and calls
   slot path for `Slot`.
2. Registry method remains the public entry for both kinds.

Update `apply_edit_batch` to match.

`apply_asset_op`:

```rust
match op {
    AssetEdit::Delete => slot_overlay.apply_delete(path),
    AssetEdit::ReplaceBody(text) => slot_overlay.apply_bytes(path, text.into_bytes()),
}
```

### `slot_apply.rs`

- Change signatures: `&EditOp` → `&SlotEdit`, `&[EditOp]` → `&[SlotEdit]`
- Remove arm: `EditOp::Delete | EditOp::SetBytes(_) => Err(UnsupportedOp)`
- `apply_op_to_def` matches only `SlotEdit` variants

### `node_def_registry.rs`

```rust
pub fn apply_artifact_edit(&mut self, change: &ArtifactEdit, ...) {
    let path = self.resolve_edit_target(...)?;
    match change {
        ArtifactEdit::Asset { ops, .. } => {
            for op in ops {
                apply_asset_op(&mut self.slot_overlay, path.clone(), op)?;
            }
        }
        ArtifactEdit::Slot { ops, .. } => {
            for op in ops {
                self.apply_slot_op(path.clone(), op, fs, ctx, frame)?;
            }
        }
    }
}
```

Remove per-op `match` on `EditOp`.

### `SyncOp::Apply(ArtifactEdit)`

No signature change — compiles once call sites updated in phase 04.

## Validate

```bash
cargo check -p lpc-node-registry
```

Full tests may fail until phase 04 updates call sites.
