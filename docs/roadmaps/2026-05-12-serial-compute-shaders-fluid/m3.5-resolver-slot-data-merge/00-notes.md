# M3.5 Resolver Slot Data And Merge Notes

## Scope

Plan the resolver upgrade needed between M3 compute shader nodes and M4 fluid
nodes.

The immediate goal is to let a produced aggregate slot, especially
`ComputeShaderNode.state.emitters: SlotData::Map`, flow through bindings into a
receiver such as a future `FluidNode.emitters` slot. The resolver should stay
shape-aware, cacheable, explainable, and embedded-friendly.

This plan is not the fluid node itself. It prepares the dataflow semantics that
will let the fluid node consume emitter maps naturally.

## Current State

### Compute Outputs

- `ComputeShaderNode` now materializes authored produced outputs into dynamic
  runtime state.
- Produced value slots become `SlotData::Value(WithRevision<LpValue>)`.
- Produced sentinel-array map slots become `SlotData::Map(SlotMapDyn)`.
- The map conversion happens in `lpc-engine`, not `lp-shader`.
- M3 summary explicitly records the current gap:
  `Production` still carries only `WithRevision<LpValue>`.

Relevant files:

- `lp-core/lpc-engine/src/nodes/shader/compute_shader_node.rs`
- `lp-core/lpc-engine/src/nodes/shader/compute_shader_state.rs`
- `lp-core/lpc-engine/src/nodes/shader/compute_materialize.rs`
- `docs/roadmaps/2026-05-12-serial-compute-shaders-fluid/m3-compute-shader-node/summary.md`

### Resolver Payloads

- `Production` is currently a leaf value:
  `Rc<WithRevision<LpValue>> + ProductionSource`.
- `QueryKey` names `ProducedSlot`, `ConsumedSlot`, `ConsumedSlotAccessor`, and
  `Bus`.
- `ResolverCache` stores `(QueryKey, Production)` in a small linear vec.
- `EngineSession::resolve` returns `Production`.
- `TickContext::resolve_consumed_slot_value<T>` assumes the resolved slot is a
  leaf `LpValue`.

Relevant files:

- `lp-core/lpc-engine/src/dataflow/resolver/production.rs`
- `lp-core/lpc-engine/src/dataflow/resolver/query_key.rs`
- `lp-core/lpc-engine/src/dataflow/resolver/resolve_session.rs`
- `lp-core/lpc-engine/src/dataflow/resolver/resolver_cache.rs`
- `lp-core/lpc-engine/src/dataflow/resolver/tick_resolver.rs`
- `lp-core/lpc-engine/src/node/contexts.rs`

### Binding Model

- Runtime bindings live on `NodeTree`.
- A binding source is one of:
  - `Literal(LpValue)`
  - `ProducedSlot { node, slot }`
  - `BusChannel(ChannelName)`
- A binding target is one of:
  - `ConsumedSlot { node, slot }`
  - `BusChannel(ChannelName)`
- Consumed bindings are indexed by exact `(NodeId, SlotPath)`.
- Bus providers are indexed by `ChannelName`.
- Bus conflicts are currently resolved by priority; duplicate priorities are
  rejected.
- For consumed slots with multiple bindings, owner closest to root wins.

Relevant files:

- `lp-core/lpc-engine/src/dataflow/binding/binding_entry.rs`
- `lp-core/lpc-engine/src/node/node_tree.rs`
- `lp-core/lpc-engine/src/node/node_binding_index.rs`
- `lp-core/lpc-model/src/binding/*`

### Slot Model

- `SlotData` already represents dynamic owned snapshots:
  `Unit`, `Value`, `Record`, `Map`, `Enum`, `Option`.
- `SlotDataAccess` already lets Rust-authored structs expose the same tree
  without allocating `SlotData`.
- `lookup_slot_data` can navigate through a slot root using `SlotPath` and a
  `SlotShapeRegistry`.
- Slot maps are keyed by typed `SlotMapKey`; keys are not JSON object field
  names.
- Slot paths can address map keys using bracket notation, e.g.
  `emitters[7]`.

Relevant files:

- `lp-core/lpc-model/src/slot/slot_data.rs`
- `lp-core/lpc-model/src/slot/slot_access.rs`
- `lp-core/lpc-model/src/slot/slot_lookup.rs`
- `lp-core/lpc-model/src/slot/slot_path.rs`

### Engine Host Fallbacks

- `EngineResolveHost::read_runtime_state_product` reads runtime state but
  rejects anything other than `SlotDataAccess::Value`.
- `EngineResolveHost::read_authored_def_product` and
  `read_authored_def_product_by_accessor` do the same for authored defaults.
- This is the direct place where aggregate slot resolution currently stops.

Relevant file:

- `lp-core/lpc-engine/src/engine/engine.rs`

## User Notes That Should Influence The Plan

- `lp-shader` must know nothing about maps.
- Maps, merge semantics, and slot revisions are LightPlayer dataflow concepts.
- Bindings need to happen at the slot level, including aggregate slots.
- The receiver should own merge strategy, not individual bindings.
- `merge = "by_key"` / `merge = "latest"` / `merge = "error"` is the preferred
  vocabulary so far.
- A slot probe/explain flow should eventually show how conflicts and merges
  were handled.
- Do not overbuild a general “everything language”; this should solve the
  immediate aggregate dataflow pressure without making fluid depend on hacks.
- Embedded constraints matter: keep cache/storage compact and avoid unnecessary
  allocations.
- The resolver should not become a second long-lived owner of node data. Nodes,
  authored defs, resources, and buses own durable state; the resolver owns only
  same-frame resolved answers, cache entries, and merge outputs.
- It is acceptable to think of the resolver as owning resolved data
  internally, but profiling already showed resolver-centric `memcpy` cost. The
  design should keep owned resolved answers simple while making aggregate copies
  bounded, same-frame, and easy to profile.

## Data Ownership Sketch

Durable data lives outside the resolver:

- Authored node defs live in the artifact store.
- Runtime-produced node state lives on node runtimes.
- Runtime buffers/textures/resources live in their resource stores.
- Binding indexes live on the node tree.

The resolver is a per-frame/session coordinator:

- It asks the host to read authored defaults or produced runtime slots.
- It asks the host to tick producer nodes on demand.
- It caches resolved answers for the current frame only.
- It materializes owned data only at resolution boundaries where it needs a
  stable cache entry or a merged result.

The desired implementation shape is therefore:

- Leaf scalar/value resolution remains cheap and mostly clone-by-value.
- Produced aggregate slots are read from node-owned state through
  `SlotDataAccess` and copied only when they become a cached `ResolvedSlot`.
- Merge operations allocate a new owned `SlotData` for the merged answer,
  because the merged result is not owned by any single source node.
- Large opaque payloads should stay as `LpValue` resource/product handles, not
  inline data. This keeps slot aggregation from accidentally copying textures,
  buffers, or rendered products.

This keeps the resolver as the owner of *answers*, not the owner of the
underlying data model.

## Answered Questions

### A1: Resolved data ownership

Use owned resolved answers in the resolver cache. Do not try to cache borrowed
`SlotDataAccess` views in M3.5.

Rationale:

- Borrowed cache entries would make node/artifact lifetimes and re-entrant
  resolution substantially harder to reason about.
- Owned answers are easier to trace, merge, test, and hand to consumers.
- Large data should be represented by `LpValue` handles, not copied through the
  resolver, so owned answers should remain bounded.
- Merge outputs are necessarily owned derived data because no single producer
  owns the merged result.

Follow-up:

- Keep an eye on resolver copy cost. If aggregate slots become large or hot,
  optimize the cache payload with `Rc<SlotData>` / small-copy helpers or
  specialized merge paths, but only after this semantic pass is correct.

## Open Questions

### Q1: What Is The New Resolved Payload Type?

Current context:

- `Production` means “resolved leaf `LpValue` plus provenance.”
- Aggregate resolution needs to carry `SlotData` or borrowed `SlotDataAccess`.
- Caching borrowed access is hard because runtime state is borrowed from nodes
  only during host production; owned `SlotData` is easier to cache and trace.

Suggested answer:

- Rename or evolve `Production` into `ResolvedSlot`.
- Store owned `SlotData` plus source/provenance.
- Add helper APIs for the common leaf case:
  - `as_value() -> Option<&WithRevision<LpValue>>`
  - `into_value() -> Result<WithRevision<LpValue>, ...>`
  - `as_lps_value_f32()` for shader-compatible leaf values.
- Keep the resolver cache as `(QueryKey, ResolvedSlot)`.

Tradeoff:

- Cloning owned `SlotData` can allocate for maps/records, but aggregate slots
  are exactly the cases where an owned cached answer is useful and explainable.
  We can optimize later with `Rc<SlotData>` or borrowed snapshots if profiling
  demands it.

### Q2: Should `QueryKey` Continue To Mean One Slot, Leaf Or Aggregate?

Current context:

- `QueryKey::ProducedSlot` and `QueryKey::ConsumedSlot` already use `SlotPath`.
- `SlotPath` can address either aggregate slots or leaf slots.
- `ConsumedSlotAccessor` is leaf-oriented because it compiles a value accessor.

Suggested answer:

- Keep `ProducedSlot` and `ConsumedSlot` as the generic aggregate-capable query.
- Keep `ConsumedSlotAccessor` as a leaf convenience for generated views.
- Make accessor resolution project from the aggregate resolver result rather
  than becoming a separate semantic path long-term.

Tradeoff:

- Some call sites will still want a leaf-only API; keep them as helpers on top
  of aggregate resolution rather than splitting the core resolver.

### Q3: Where Does Merge Strategy Live?

Current context:

- User prefers receiver-owned merge strategy.
- The current binding model has no slot metadata for merge strategy.
- `ShaderSlotDef` has authored slot defs, but generic receiver node defs do not
  yet have per-slot metadata beyond their typed fields/shapes.

Suggested answer:

- Add a small `SlotMerge` enum in `lpc-model`, probably near binding or slot
  model:
  - `error`
  - `latest`
  - `by_key`
- Attach merge policy to consumed slot definitions where dynamic slot defs
  exist (`ShaderSlotDef`/future `FluidDef` fields).
- For Rust-authored fixed slots, use a runtime trait or a small per-node method
  such as `NodeRuntime::merge_policy(slot: &SlotPath)`.
- For M3.5, implement policy lookup in the engine host with a conservative
  default:
  - leaf/value slots: `latest` for current single-binding behavior
  - aggregate map slots: `error` unless explicitly `by_key`

Tradeoff:

- Putting merge policy only in `SlotShape` feels tempting, but shape is static
  structure; merge is receiver behavior. It may need metadata attached to slot
  definitions, not arbitrary shape nodes.

### Q4: How Do Bus Bindings Handle Multiple Aggregate Providers?

Current context:

- Bus provider selection currently chooses a single highest-priority provider.
- Duplicate provider priority is rejected at binding-index rebuild time.
- Map merging implies multiple providers can contribute to one receiver.
- Bus channels are idiomatic decoupling points for visuals/data.

Suggested answer:

- Keep existing priority behavior for scalar/leaf values.
- For mergeable aggregate receivers, allow the receiver resolution path to ask
  for all bus providers, not just the highest-priority provider.
- Do not globally relax duplicate-priority bus validation until the target
  receiver explicitly asks for merge semantics.
- M3.5 can implement direct node/bus aggregate merge in a focused path and keep
  old scalar behavior intact.

Tradeoff:

- A bus channel does not know its eventual receiver policy in isolation. The
  resolver may need “resolve bus for receiver slot” context before selecting
  providers.

### Q5: Do Bindings Target Exact Slots Or Can They Target Descendants?

Current context:

- Bindings are indexed by exact consumed target path.
- A binding to `emitters` should satisfy a request for `emitters[7]` if the
  result is a map.
- Conversely, a binding to `emitters[7]` may need to contribute to the aggregate
  `emitters` later.

Suggested answer:

- M3.5 should support exact aggregate slot binding first:
  `emitters` resolves as a map.
- Leaf/descendant projection can be layered on top:
  resolve `emitters`, then look up `[7]` in the resolved `SlotData`.
- Do not implement reverse assembly from many child bindings into a parent map
  yet unless the fluid slice genuinely needs it.

Tradeoff:

- This keeps the first implementation small while preserving the mental model
  that slots, not value paths, are bindable.

### Q6: What Should Explain/Trace Capture Now?

Current context:

- `ResolveTrace` records cache hits, binding selection, produce start/end, and
  errors.
- It does not capture merge steps or multiple provider decisions.

Suggested answer:

- Add minimal merge trace events now:
  - merge policy selected
  - merge input accepted/replaced/skipped
  - merge conflict/error
- Do not build a full UI explain probe in M3.5, but shape the trace events so
  `ExplainSlotProbe` can use them later.

Tradeoff:

- Trace event strings/variants become part of our debugging vocabulary, so keep
  them semantic rather than over-specific.

### Q7: How Much Real Fluid Prep Belongs Here?

Current context:

- M4 fluid wants to consume emitter data.
- `FluidEmitter` exists as a native value shape.
- No `FluidDef` or `FluidNode` exists yet.

Suggested answer:

- Add a test-only or minimal fake receiver node in `lpc-engine` tests that
  consumes a map slot with `merge = "by_key"`.
- Do not add real `FluidNode` in this plan.
- The phase should end with an engine test equivalent to:

  ```text
  compute_a.output emitters -> bus#fluid.emitters
  compute_b.output emitters -> bus#fluid.emitters
  receiver.emitters source = bus#fluid.emitters
  resolver returns one merged SlotData::Map keyed by emitter id
  ```

Tradeoff:

- A fake receiver keeps this about resolver semantics. The real fluid node can
  then be much smaller and less speculative.
