## Wire sync for ArtifactOverlay

- **Idea:** Expose overlay roots via `slot_sync_codec` / `WireSlotPatch` per artifact or one registry root.
- **Why not now:** Registry shape must land first; M1 wire design still open.
- **Useful context:** `lpc-wire/src/slot/access_sync.rs`, M1 `ui-parity.md` pending row.

## MapSlotKeyLike for SlotPath

- **Idea:** Use `MapSlot<SlotPath, SlotEdit>` directly instead of string keys.
- **Why not now:** Requires `lpc-model` change; string keys sufficient for v1.
- **Useful context:** `lpc-model/src/slot/value_slot.rs` `MapSlotKeyLike`.

## SessionLog (M8) for ordering / audit

- **Idea:** Optional append-only log above the pending map for multi-client ordering.
- **Why not now:** Address-keyed map + revision CAS is enough for v1 single editor.
- **Useful context:** `m8-edit-session-sync/00-design.md` — explicitly superseded for overlay storage.

## Cached effective projection (per artifact)

- **Idea:** After folding `committed + pending → NodeDef`, cache the result keyed by
  `ArtifactLocation` (or on each affected `NodeDefEntry`). Path lookups (`effective_state`,
  `NodeDefView`, slot accessors) hit the cache instead of re-cloning and re-applying the
  pending map on every read.
- **Invalidation:** Clear or rebuild cache when that artifact's `ArtifactPending` bucket
  changes (any upsert/remove on apply, or `remove_pending_edit` / `discard` / successful
  commit for that location). Committed fs sync that changes the base def also invalidates.
- **Why not v1:** Ephemeral fold in `projection.rs` is simpler and has no invalidation bugs;
  pending maps are usually small so per-read cost may be fine initially.
- **API shape:** Phase 3 should keep projection behind `project_*` helpers so a cache layer
  can wrap them without changing `NodeDefView` callers.
- **Useful context:** User agreed v1 = no stored effective; caching is an optimization for
  hot path reads, not a second source of truth (overlay map remains authoritative).
