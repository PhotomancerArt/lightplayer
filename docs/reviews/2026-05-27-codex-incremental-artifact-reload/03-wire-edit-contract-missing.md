# Wire Edit Contract Missing

- **Severity:** P1
- **Status:** open
- **First seen:** 2026-06-10-api-design-review.md
- **Last reviewed:** 2026-06-10-api-design-review.md
- **Owner:** unassigned

## Finding

The new registry edit model is not represented by a durable wire contract. `SlotEdit` is serde-ready locally, but the registry ingress enum mixes client intent with server-local filesystem notifications, and the existing `lpc-wire` mutation API is still the old value-leaf-only engine mutation path. That means we cannot currently write the requested "using on-wire API" registry test for add/delete node, structural def edits, asset CRUD, pending overlay, and commit.

## Evidence

- `lp-core/lpc-node-registry/src/registry/sync_op.rs:7` - `SyncOp` is documented as "filesystem or pending-edit CRUD", combining server-local `Fs(FsEvent)` with client-ish edit/commit operations.
- `lp-core/lpc-node-registry/src/registry/sync_op.rs:8` - `SyncOp` derives `Clone`, `Debug`, and `PartialEq`, but not serde/schema traits for wire use.
- `lp-core/lpc-node-registry/src/registry/sync_op.rs:10` - `SyncOp::Fs` carries `lpfs::FsEvent`, which should not be client wire vocabulary.
- `lp-core/lpc-node-registry/src/edit_model/slot_edit.rs:6` - `SlotEdit` itself derives serde, but it is only the slot-level operation, not an edit request envelope with target, batch id, expected revision, commit/discard intent, or response semantics.
- `lp-core/lpc-node-registry/src/edit_model/artifact_overlay.rs:28` - `AssetEdit` is not currently a wire/schema type.
- `lp-core/lpc-wire/src/slot/mutation.rs:24` - `WireSlotMutationRequest` still addresses a string `root` plus `SlotPath` with shape/data CAS revisions.
- `lp-core/lpc-wire/src/slot/mutation.rs:37` - `WireSlotMutationOp` only supports `SetValue`.
- `lp-core/lpc-wire/src/messages/project_read/project_read_request.rs:25` - project reads still carry `Vec<WireSlotMutationRequest>`, not registry edit batches.
- `lp-core/lpc-engine/src/engine/slot_mutation.rs:19` - the current server-facing mutation path still mutates `Engine` state directly from `WireSlotMutationRequest`.

## Impact

This blocks using the registry as the authoritative edit layer over the wire. A client cannot express asset creation/deletion, node add/remove, structural `EnsurePresent`, overlay discard/commit, or registry commit results through the current protocol. If `SyncOp` is promoted directly, it will leak filesystem events and registry-local details onto the wire before revision, idempotency, and response semantics are decided.

## Suggested Fix

Define a wire-facing edit contract separately from registry internals, then add a small adapter into `NodeDefRegistry`.

Suggested shape:

- `ClientEditRequest { id, ops, base_revision?, commit_policy? }`
- `ClientEditOp::UpsertSlot { artifact_path, edit: SlotEdit }`
- `ClientEditOp::SetAsset { artifact_path, edit: AssetEdit }`
- `ClientEditOp::RemovePending { artifact_path }`
- `ClientEditOp::ClearPending`
- `ClientEditOp::Commit`
- `ClientEditResponse { id, accepted/rejected, pending_changed, committed: SyncResult?, current_revision? }`

Keep `FsEvent` and filesystem sync as server-local registry API. Map wire requests to registry operations inside server code.

## Validation

- Add `lpc-wire` serde/schema roundtrip tests for the new edit request/response.
- Add a registry adapter test that applies the wire request sequence and asserts the same `ArtifactOverlay`/`SyncOutcome` as direct registry calls.
- Add a negative test proving clients cannot send filesystem events.

## History

- 2026-06-10: opened by Codex API design review.
