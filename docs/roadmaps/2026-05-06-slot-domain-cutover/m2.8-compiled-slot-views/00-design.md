# M2.8 Compiled Slot Views Design

## Scope

M2.8 introduces compiled slot accessors and generated read-only slot views. It keeps `SlotPath` as the authored, wire, and diagnostic path format, but runtime code should not repeatedly scan record field names for hot reads.

Out of scope:

- Mutable slot views.
- Client mutation API.
- Full map/enum/option accessor optimization.
- Applying generated views across every node def beyond the texture slice needed to prove the pattern.

## File Structure

```text
lp-core/lpc-model/src/slot/
  slot_accessor.rs          # compiled, registry-revision-checked slot lookup
  slot_lookup.rs            # keeps path lookup; may delegate shared helpers
  slot_shape_registry.rs    # exposes revision helper if needed

lp-core/lpc-engine/src/slot_view/
  mod.rs
  texture_def_view.rs       # generated or generated-compatible view wrapper

lp-core/lpc-slot-macros/src/
  record.rs                 # emits generated view type for root records
  attr.rs                   # view-related attrs only if needed

lp-core/lpc-model/tests/
  slot_accessor.rs          # direct accessor compile/use tests
  slot_record_derive.rs     # view generation tests
```

## Architecture

`SlotPath` remains the semantic address. `SlotAccessor` is the compiled address.

The accessor is compiled against a root shape id and a registry snapshot revision. During compilation, record field names are resolved to field indices and referenced shapes are followed. During access, record traversal uses `SlotRecordAccess::field(index)`.

```text
SlotPath + root shape + SlotShapeRegistry
          │
          ▼
    SlotAccessor
      root_shape_id
      registry_revision
      original_path
      indexed_steps
          │
          ▼
  SlotAccessor::get(root, registry)
          │
          ▼
    SlotDataAccess
```

`TickContext` gains a handle-based method:

```rust
resolve_consumed_slot_accessor_value<T>(&mut self, accessor: &SlotAccessor) -> Result<T, NodeError>
```

For consumed slots, the resolver still accepts `QueryKey::ConsumedSlot { slot: SlotPath }` initially, so the accessor stores the original path. The performance gain in M2.8 is on authored default lookup and typed view construction. A later resolver cleanup can make `QueryKey` itself accessor-aware.

Generated views compile their accessors against a registry revision and expose typed methods:

```rust
let view = TextureDefView::compile(engine.slot_shapes())?;
let size = view.size(&mut ctx)?;
```

For now the engine can construct these cheaply per tick and internally reuse a small cache keyed by registry revision later. The main contract is that accessors are invalidated when the registry revision changes.

## Interactions

- `SlotShapeRegistry` is the source of shape truth and cache invalidation.
- `SlotAccessor` owns the original path for diagnostics and resolver compatibility.
- Generated views own accessors, not borrowed registry references.
- `TickContext` remains the resolver boundary for node config reads.

