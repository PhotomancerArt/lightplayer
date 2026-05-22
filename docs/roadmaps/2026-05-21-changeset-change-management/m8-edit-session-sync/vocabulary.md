# Edit Session Vocabulary

Canonical names for M8 and beyond. Layer 0 (`FsEvent`) and layers 3–4 (`SyncOp`,
`SyncOutcome`) land in later M8 phases.

## Layer 1 — Edit vocabulary (`lpc-node-registry/src/edit/`)

| Old | New |
|-----|-----|
| `change/` module | `edit/` |
| `ArtifactOp` | `EditOp` |
| `ArtifactChange` | `ArtifactEdit` |
| `ChangeSet` | `EditBatch` |
| `ChangeSetId` | `EditBatchId` |
| `ArtifactTarget` | `EditTarget` |
| `ChangeError` | `EditError` |

`EditBatch` field: `edits: Vec<ArtifactEdit>` (serde alias `changes` for wire compat).

## Layer 2 — Slot overlay (registry pending state)

| Old | New |
|-----|-----|
| `ChangeOverlay` | `SlotOverlay` |
| `OverlayEntry` | `SlotOverlayEntry` |
| `SlotDraft` | `DefDraft` |
| `NodeDefRegistry.overlay` | `NodeDefRegistry.slot_overlay` |
| `apply_changeset` | `apply_edit_batch` |
| `apply_change` | `apply_artifact_edit` |
| `discard_overlay` | `discard_slot_overlay` |
| `overlay_active` | `slot_overlay_active` |
| `overlay_contains_path` | `slot_overlay_contains_path` |
| `overlay_bytes` | `slot_overlay_bytes` |

## Legacy aliases

Deprecated type aliases live in `edit/mod.rs` and `lib.rs` (`change` module re-export).

## Not yet renamed (later M8 phases)

- `RegistryChange` → `SyncOp`
- `FsChange` → `FsEvent`
- Session log types (`SessionVersion`, `SessionEvent`, …)
