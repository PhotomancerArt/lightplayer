# Produced Slots Runtime Cleanup Design

## Scope

This plan is an engine/runtime cleanup slice. It establishes the core produced/consumed slot vocabulary and removes misleading legacy runtime surfaces, without redesigning the wire/view layer or final authored binding syntax.

In scope:

- Make the new demand-driven `Node` trait the obvious runtime spine.
- Delete or quarantine the old `NodeRuntime` trait and old legacy runtime files if they are no longer used by the project loader.
- Split slot identity from value traversal with `SlotName`, `SlotOwner`, `SlotRef`, and `ValueRef` model types.
- Keep `ValuePath` as the parsed `Vec<Segment>` path inside a structured value.
- Keep authored strings as input formats where possible; parse into semantic types at source/input boundaries.
- Replace `RuntimePropAccess` and `RuntimeOutputAccess` with one produced-slot access surface returning `RuntimeProduct`.
- Rename resolver and binding concepts from input/output language to consumed/produced slot language.
- Remove `PropNamespace` from core semantic validation.
- Update rustdocs and tests so the new terms are the documented norm.

Out of scope:

- Final generic wire/view data model.
- Final binding/value-reference text syntax.
- Declarative slot metadata for all node kinds.
- Source reload lifecycle.
- Removing texture artifacts/nodes.

## File Structure

```text
lp-core/lpc-model/src/
  node/
    relative_node_ref.rs     # parsed relative node refs plus source helper if needed
  prop/
    value_path.rs            # parsed field/index traversal inside values
  slot/
    mod.rs
    slot_name.rs             # named slot in a slot owner's namespace
    slot_owner.rs            # Node or Bus owner identity
    slot_ref.rs              # owner + slot
    value_ref.rs             # slot ref + value path

lp-core/lpc-engine/src/
  node/
    node.rs                  # the single runtime node trait
    contexts.rs              # consumed-slot resolver access in tick contexts
  prop/
    produced_slot_access.rs  # produced-slot access trait
    runtime_product.rs       # produced payload type, or current module export
  resolver/
    query_key.rs             # Bus / ConsumedSlot / ProducedSlot
    production.rs            # provenance renamed to produced-slot terms
    resolve_session.rs       # binding-aware consumed resolution
  binding/
    binding_entry.rs         # ProducedSlot sources, ConsumedSlot targets
```

The exact module names can follow local crate organization during implementation, but each concept should have one obvious home.

## Architecture Summary

Nodes and buses own slot namespaces. A slot is a named location on one owner. A value path is a field/index traversal inside the value currently exposed at a slot.

```text
SlotOwner + SlotName = SlotRef
SlotRef + ValuePath  = ValueRef
```

Direction is not part of `SlotRef` or `ValueRef`. Direction is part of the operation:

- A produced-slot read asks a runtime node for data it owns and writes.
- A consumed-slot read goes through the resolver because bindings, defaults, priorities, buses, tracing, and cycle detection all participate.

Bindings are intended to happen at the slot level. `ValueRef` can name nested data for reads, projection, and diffs, but a nested value path is not its own version boundary.

`RuntimeProduct` remains the produced payload because it can represent both direct values and engine-owned handles:

```rust
RuntimeProduct::Value(LpsValueF32)
RuntimeProduct::Render(RenderProductId)
RuntimeProduct::Buffer(RuntimeBufferId)
```

The produced access trait should be the only node-owned produced-data surface. It replaces the current representation split between scalar `RuntimePropAccess` and non-scalar `RuntimeOutputAccess`.

## Main Concepts

### Relative Node References

`RelativeNodeRef` is a parsed, context-dependent source/model reference. It is not a runtime `NodeRef` because it needs a current node to resolve. `RelativeNodeRefSrc` may remain only where serde or source diagnostics require raw authored text.

A future `NodeRef` should mean resolved runtime identity, probably a `NodeId`, `TreePath`, or wrapper over both.

### Value Paths

`ValuePath` is the parsed path inside a structured value. It is not a slot identifier and should not be used as one.

Examples:

```text
image.width
touches[1].x
diagnostics.compile_ms
```

### Slot References

`SlotName` identifies one slot within an owner namespace. In this plan it is an opaque string key and may contain names such as `config.width`; structured slot paths are future work.

`SlotOwner` abstracts the two current slot namespace owners:

```rust
enum SlotOwner {
    Node(NodeId or node reference type),
    Bus(ChannelName or bus reference type),
}
```

A bus is a slot owner, not a node. It has routing semantics, not node lifecycle or tick behavior.

### Produced Access

Runtime nodes expose produced data through one trait, tentatively:

```rust
trait ProducedSlotAccess {
    fn get(&self, path: &ValuePath) -> Option<(RuntimeProduct, FrameId)>;
    fn iter_changed_since(&self, since: FrameId) -> Box<dyn Iterator<Item = (ValuePath, RuntimeProduct, FrameId)> + '_>;
    fn snapshot(&self) -> Box<dyn Iterator<Item = (ValuePath, RuntimeProduct, FrameId)> + '_>;
}
```

The implementation keeps `ValuePath`-shaped slot keys as a compatibility bridge.
The important rule is that the trait returns `RuntimeProduct` and covers both
scalar and resource-handle products.

### Consumed Resolution

Nodes read consumed values through `TickContext::resolve(...)`. Resolver queries should say what they mean:

```rust
enum QueryKey {
    Bus(...),
    ConsumedSlot { node: NodeId, slot: ValuePath },
    ProducedSlot { node: NodeId, slot: ValuePath },
}
```

The implementation may temporarily adapt old whole-path values while introducing `SlotName`, but new names and rustdocs should not describe `input` or `output` as fixed namespaces.

### Bindings

Bindings should describe direction by role and should use slot-level endpoints:

```rust
enum BindingSource {
    Literal(SrcValueSpec),
    ProducedSlot { node: NodeId, slot: ValuePath },
    BusSlot(...),
}

enum BindingTarget {
    ConsumedSlot { node: NodeId, slot: ValuePath },
    BusSlot(...),
}
```

A binding target should not normally be a produced node slot, because produced slots are owned by the producer node. If an external writer pattern is needed later, it should get an explicit name rather than reusing produced-slot target semantics.

Structured slot data is future work. A later plan should introduce the `SlotValue` / `SlotPath` story so `state.touches` can be versioned as one slot while still allowing `state.touches[3].id` to be inspected or diffed as nested data.

## Compatibility Boundary

The wire/view system can continue using compatibility projections. This plan should adjust compile errors caused by runtime renames, but should not attempt to design the future generic client data model.

`PropNamespace` should be removed from core validation. If existing compatibility code needs conventional root names such as `state` or `param`, those should be local conventions or temporary adapters, not semantic proof that a slot is consumed or produced.

## Validation Approach

Use targeted host tests for crates touched by each phase. Do not run `cargo test --workspace` or `cargo build --workspace`.

Likely final validation:

```bash
cargo test -p lpc-model
cargo test -p lpc-source
cargo test -p lpc-engine
cargo test -p lpc-view
cargo test -p lpc-wire
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```

If a phase touches shader pipeline behavior, add the AGENTS.md shader validation commands for that phase.
