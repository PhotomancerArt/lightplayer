# Future Work

## Accessor-Aware Resolver Keys

- **Idea:** Let `QueryKey::ConsumedSlot` and `QueryKey::ProducedSlot` carry compiled accessors or compact slot ids instead of `SlotPath`.
- **Why not now:** The resolver API still uses semantic paths broadly; changing it now would widen M2.8 too much.
- **Useful context:** M2.8 keeps the original path inside `SlotAccessor` so this migration remains possible.

## Finer Shape Invalidation

- **Idea:** Cache accessors against the root shape entry revision and referenced shape revisions instead of the global registry id revision.
- **Why not now:** Global registry revision invalidation is simpler and safe. Shape changes should be rare.
- **Useful context:** `SlotShapeRegistry::entry` already exposes per-root `WithRevision<SlotShape>`.

## Full Container Accessor Support

- **Idea:** Compile map, enum, and option steps into specialized accessor steps.
- **Why not now:** The immediate node-config slice needs record-to-value access. Container support should be added when a concrete def field needs it.
- **Useful context:** `SlotPathSegment::Key` and existing `lookup_slot_data` define the semantics.

