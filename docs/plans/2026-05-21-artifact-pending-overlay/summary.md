# Artifact Pending Overlay — Implementation Summary

## What changed

Replaced path-keyed `SlotOverlay` / `DefDraft` materialization with address-keyed
`ArtifactOverlay` storing **current pending edits** per `ArtifactLoc`:

- **Slot pending:** `MapSlot<String, SlotEdit>` with upsert-by-edit-key and apply-order
  tracking (supports multiple `MapInsert` ops on the same map path).
- **Asset pending:** `AssetPending` (`None` | `Delete` | `ReplaceBody`) mutually exclusive
  with slot map.
- **Projection:** `registry/projection.rs` folds committed + pending on read (in-memory
  for loaded defs; bytes path for commit / effective bytes).
- **Commit:** folds pending map → filesystem writes via `OverlayCommitPlan`.
- **Introspection:** `overlay_active`, `pending_at`, `iter_pending`, `has_pending_slot`.

## Removed

- `edit/def_draft.rs`
- `edit/slot_overlay.rs`
- Public exports: `DefDraft`, `SlotOverlay`, `SlotOverlayEntry`

## Key files

| File | Role |
|------|------|
| `edit/artifact_overlay.rs` | Overlay storage + mutual exclusion |
| `edit/pending_slot_key.rs` | `slot_path_key`, `slot_edit_key`, parse |
| `registry/projection.rs` | Fold committed + pending |
| `registry/effective_read.rs` | Effective reads delegate to projection |
| `registry/commit.rs` | Promote overlay → fs |
| `registry/slot_apply.rs` | Upsert slot ops (no fork draft) |

## Deviations from plan

1. **Outer overlay map** uses `MapSlot<String, ArtifactPending>` keyed by
   `ArtifactLoc::to_uri()` (not `MapSlot<ArtifactLoc, _>`) because `ArtifactLoc` does not
   implement `MapSlotKeyLike`.
2. **Slot edit keys** use `slot_edit_key()` (path + op kind + map key for map ops), not
   path-only — required for multiple map inserts on one path.
3. **Apply order** preserved via private `slot_order: Vec<String>`, not sorted keys.
4. **Inline def projection** always folds from **artifact root** entry, then slices
   `NodeDefLoc.path` (fixes child effective view).

## Validation

```bash
cargo test -p lpc-node-registry          # 86 tests pass
cargo check -p lpc-node-registry --no-default-features
cargo +nightly fmt -p lpc-node-registry
```

## Follow-ups (see `future.md`)

- Wire sync / client read-back of pending map
- Per-artifact cached effective projection
- `SessionLog` integration (M8)
