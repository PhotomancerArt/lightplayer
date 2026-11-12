# Phase 4: Produced Resolution Bridge

## Scope Of Phase

Make engine produced-slot resolution read from runtime state slots.

In scope:

- Add a helper for reading a produced runtime product from `NodeRuntime::runtime_state_slots()`.
- Convert `LpValue::RenderProduct` to `RuntimeProduct::Render`.
- Preserve existing scalar produced tests by converting scalar `LpValue` values.
- Remove `ProducedSlotAccess`.

Out of scope:

- Full generic `SlotDataAccess` snapshot/diff for runtime state.
- Client/watch sync.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-model/src/slot/slot_lookup.rs`
- `lp-core/lpc-engine/src/engine/test_support.rs`
- `lp-core/lpc-engine/src/nodes/texture/texture_node.rs`

The resolver needs shape context. `SlotAccess` exposes a root
`shape_id()` plus data access; resolving a textual `SlotPath` into record indexes requires the
corresponding `SlotShape` from a registry. Do not hand-code field indexes in the engine bridge just
to make this phase pass.

The likely shape is:

```rust
fn produced_get(
    node: &dyn NodeRuntime,
    shapes: &SlotShapeRegistry,
    path: &SlotPath,
) -> Option<(RuntimeProduct, Revision)>
```

It should read a value leaf at `path` from the node state root, use the registry to walk record/map
structure, and convert supported `LpValue`s to `RuntimeProduct`.

## Validate

```bash
cargo test -p lpc-engine engine::
cargo test -p lpc-engine nodes::texture::
```
