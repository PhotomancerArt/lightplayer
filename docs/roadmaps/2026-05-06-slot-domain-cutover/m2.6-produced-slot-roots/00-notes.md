# M2.6 Runtime State Slot Roots Notes

## Scope

Replace the old "produced slots are their own mini API" idea with a cleaner runtime state model:

- A runtime node owns a public state object.
- That state object is a normal slot root, usually `#[derive(SlotRecord)] #[slot(root)]`.
- Produced values are read from that state root by direction-aware engine logic.

The immediate slice is shader output:

```rust
pub struct ShaderNode {
    ...
    state: ShaderState,
}

#[derive(SlotRecord)]
#[slot(root)]
pub struct ShaderState {
    pub output: RenderProductSlot,
}
```

In scope:

- Promote graph `RenderProduct { node, output }` into `lpc-model`.
- Add `LpValue::RenderProduct { node, output }`.
- Add a semantic `RenderProductSlot`.
- Add/clarify `NodeRuntime::state()` as a public runtime slot root.
- Convert `ShaderNode` to own `ShaderState`.
- Keep old produced resolution as a small temporary bridge that reads `state.output`.

Out of scope:

- Full runtime editability.
- Generic UI rendering of node state.
- Full project sync rebuild.
- Replacing all runtime state/config with slot roots.
- Deleting `RuntimeProduct` entirely.
- Changing resource payload fetching.

## Current State

### Runtime Product

`lp-core/lpc-engine/src/render_product/render_product.rs` currently defines:

```rust
pub struct RenderProduct {
    node: NodeId,
    output: u32,
}
```

This is exactly the graph value we want to send over the wire. It is not a materialized texture and
not the old registry-owned `RenderProductId`.

### Portable Values

`lp-core/lpc-model/src/value/lp_value.rs` has portable `LpValue` variants for scalars, vectors,
arrays, structs, and resources.

It does not yet have a `RenderProduct` variant. That is the mismatch blocking `ShaderState.output`
from being a normal `ValueSlot<T>` or semantic slot leaf.

### Existing Slot Root Derive

The derive machinery already supports the desired pattern:

```rust
#[derive(SlotRecord)]
#[slot(root)]
struct SomeState {
    field: SomeSlot,
}
```

Examples:

- `lp-core/lpc-model/src/nodes/shader/shader_def.rs`
- `lp-core/lpc-slot-mockup/src/engine/fixture_node.rs`

### Old Produced Slot API

`lp-core/lpc-engine/src/prop/produced_slot_access.rs` still defines:

```rust
get(path)
iter_changed_since(since)
snapshot()
```

This is too much boilerplate and has no shape. The important correction is that we do not need a new
`ProducedSlotRoot`; the node's public runtime state is the slot root.

## Decisions

### Node Runtime Owns A Public State Root

- **Decision:** `NodeRuntime` should expose runtime state as a slot root.
- **Why:** The executable node object owns private engine machinery; the state object is the public,
  shaped, versioned surface.
- **Consequence:** `ShaderNode` owns `ShaderState`; eventually fixture/output/etc. can own their own
  state roots too.

### Render Product Is A Model Value

- **Decision:** Move `RenderProduct` to `lpc-model` and add `LpValue::RenderProduct`.
- **Why:** Render products are graph values that must cross the wire. They are not materialized
  resources, but clients need to inspect and request previews/materialization for them.
- **Consequence:** `ShaderState.output` can be a normal slot leaf.

### Produced Resolution Is Directional Engine Logic

- **Decision:** Keep direction outside the state object. The node state is just a namespace; the
  resolver decides that reading `state.output` is a produced-slot read.
- **Why:** Produce vs consume is about how the engine relates to a slot, not a different tree shape.
- **Consequence:** The old `ProducedSlotAccess` can become a temporary bridge and then disappear.

## Open Questions

### Q1: Should `NodeRuntime::state()` return `Option<&dyn SlotAccess>` or `&dyn SlotAccess`?

Suggested answer: use `&dyn SlotAccess` with an empty default root. This avoids optional handling
everywhere and keeps "every node has state, maybe empty" as the invariant.

### Q2: What is the field path for produced output?

Suggested answer: keep the engine-facing slot as `"output"` on the runtime state root. In UI/wire
vocabulary it may be displayed as `state.output` when disambiguating node state from config/def.

### Q3: Should `RuntimeProduct` continue to exist?

Suggested answer: yes for now. It still wraps shader ABI values (`LpsValueF32`), render products,
and buffers inside engine resolution. Once more values move into `LpValue`, this can be revisited.

