# M3.5 Resolver Slot Data And Merge Design

## Scope

Upgrade resolver payloads from leaf-only `LpValue` productions to aggregate-capable resolved slots. This allows produced map slots from compute shader nodes to flow through bindings and be merged by future receiver nodes such as fluid.

Out of scope:

- Real `FluidNode` implementation.
- Full explain/probe UI.
- Reverse assembly of aggregate slots from many child-slot bindings.
- Optimizing aggregate copies beyond bounded same-frame ownership.

## File Structure

```text
lp-core/lpc-model/src/slot/
  slot_lookup.rs      # add shape-aware lookup helper
  slot_merge.rs       # receiver merge policy enum

lp-core/lpc-engine/src/dataflow/resolver/
  production.rs       # evolve `Production` into aggregate-capable resolved slot
  resolver_cache.rs   # cache QueryKey -> Production/ResolvedSlot
  resolve_session.rs  # aggregate merge routing
  resolve_host.rs     # host hooks for all consumed bindings and merge policy
  resolve_trace.rs    # merge trace events

lp-core/lpc-engine/src/engine/
  engine.rs           # host snapshots runtime/authored slot data

docs/roadmaps/.../m3.5-resolver-slot-data-merge/
  00-notes.md
  00-design.md
  01-resolved-slot-payload.md
  02-receiver-merge-resolution.md
  03-cleanup-validation.md
```

## Architecture Summary

`Production` becomes the resolver-owned resolved answer. It carries owned `SlotData` plus provenance. Existing leaf APIs remain available as helpers, so current node call sites can still read `LpValue`/shader-compatible values without knowing whether the core payload is aggregate-capable.

Durable data remains outside the resolver:

- authored defs in the artifact store,
- runtime state on node runtimes,
- heavy resources in resource stores,
- bindings on the node tree.

The resolver snapshots slot data only when answering a query or producing a merged answer. The cache stays per-frame and is cleared at the beginning of each engine tick.

Consumed slot resolution gains receiver-owned merge policy. Normal scalar slots continue to use the current single selected binding/default behavior. Mergeable aggregate slots can collect multiple source bindings and merge them into one owned `SlotData` answer. M3.5 implements `by_key` map merge, with deterministic provider order and trace events.

