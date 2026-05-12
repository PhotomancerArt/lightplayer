# M2.8 Compiled Slot Views Notes

## Scope

Build the first general form of compiled slot accessors and generated read-only slot views.

The immediate pressure is avoiding repeated string/path lookup in node `tick()` code. Authored slot paths are still the source and wire language, but runtime access should compile those paths against the shape registry once, cache them against registry revision, and then walk record fields by index.

This milestone should:

- Add a reusable compiled accessor type in `lpc-model`.
- Cache compiled accessors against the slot shape registry revision.
- Let `TickContext` resolve consumed slots by compiled accessor.
- Generate a simple typed `*View` from `#[derive(SlotRecord)]` for root records.
- Convert the manual `TextureDefView` slice to the generated/accessor-backed model.

## Current State

- `SlotShapeRegistry` already has `ids_revision` and per-shape `WithRevision<SlotShape>` entries.
- `lookup_slot_data` is shape-aware but walks a `SlotPath` and scans record field names every time.
- `SlotRecordAccess::field(index)` is already index-based, which is the fast path we want.
- `TickContext::resolve_consumed_slot_value` currently takes `&SlotPath`.
- `TextureDefView` is manual and reparses `SlotPath::parse("size")` on every call.
- `lpc-slot-macros` already derives `SlotRecordShape`, `SlotRecordAccess`, `FieldSlot`, and root `StaticSlotShape`.

## User Direction

- Avoid looking up every slot by string every tick.
- Use records by index.
- Cache by registry version/revision and rebuild when any shape changes.
- Build the more general form now, including codegen.
- Do not go too far: quick, reasonable access, not a full optimizer.

## Suggested Answers

### Cache Granularity

Use `SlotShapeRegistry::ids_revision` as the first cache key.

This is conservative: any root add/remove/replace invalidates compiled views. That is acceptable for now because shape churn is rare relative to tick reads, and it avoids subtle stale-accessor bugs when referenced shapes change.

Later we can use per-root shape entry revision for finer invalidation if needed.

### Compiled Accessor Shape

Add a `CompiledSlotPath` or `SlotAccessor` in `lpc-model`.

It should contain:

- Root `SlotShapeId`
- Registry revision it was compiled against
- Indexed steps for record fields
- Original `SlotPath` for diagnostics

For M2.8, support the subset we use immediately:

- Record field traversal by compiled index.
- References followed at compile time.
- Value leaf validation.

Map, enum, and option steps can either be represented or left as explicit future work. The runtime config-view slice only needs record fields now.

### Codegen Shape

Extend `#[derive(SlotRecord)]` for root records to generate a sibling view type:

- `TextureDefView`
- `ShaderDefView`
- etc.

The generated view should compile field accessors once from a `SlotShapeRegistry` and expose methods for fields whose types can be read as values. It is fine if the first generated API is modest and falls back to explicit manual code for unsupported field kinds.

### Naming

Use "accessor" for the compiled address:

- `SlotAccessor`
- `SlotAccessorStep`
- `SlotAccessorError`

Use "view" for the typed ergonomic wrapper:

- generated `TextureDefView`
- resolver-backed methods on the view

## Open Questions

No blocking questions. The conservative path is enough:

- Cache against `SlotShapeRegistry::ids_revision`.
- Rebuild views on cache miss or revision mismatch.
- Keep the first accessor implementation focused on record/value reads.

