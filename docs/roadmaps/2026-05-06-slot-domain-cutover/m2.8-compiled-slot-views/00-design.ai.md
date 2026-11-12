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

lp-core/lpc-slot-codegen/src/
  lib.rs                    # discovers #[slot(root, view)] records and emits views

lp-core/lpc-slot-macros/src/
  attr.rs                   # accepts the view marker; derive still owns record access

lp-core/lpc-model/tests/
  slot_accessor.rs          # direct accessor compile/use tests
  slot_record_derive.rs     # derive behavior tests
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

Build-time generated views compile their accessors against a registry revision and expose accessor methods:

```rust
let view = TextureDefView::compile(engine.slot_shapes())?;
let size = ctx.resolve_consumed_slot_accessor_value(view.size())?;
```

The engine caches generated views on runtime nodes and rebuilds them when the registry revision changes. The main contract is that accessors are invalidated when the registry revision changes.

## Interactions

- `SlotShapeRegistry` is the source of shape truth and cache invalidation.
- `SlotAccessor` owns the original path for diagnostics and resolver compatibility.
- Generated views own accessors, not borrowed registry references.
- Generated views live in `lpc-model`; resolver-backed reads remain an engine concern.
- `TickContext` remains the resolver boundary for node config reads.
