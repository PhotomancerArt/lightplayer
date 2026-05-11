# Produced Slots Runtime Cleanup Summary

## What changed

- Established `ValuePath` as the shared parsed path for traversal inside values.
- Replaced source-side `NodeLoc` naming with `RelativeNodeRef` and parsed node
  references at source boundaries.
- Added model slot vocabulary:
  - `SlotName`
  - `SlotOwner`
  - `SlotRef`
  - `ValueRef`
- Removed the old `NodeRuntime` runtime spine and the unused legacy runtime
  files. The demand-driven `Node` trait is now the visible runtime node API.
- Replaced split `RuntimePropAccess` / `RuntimeOutputAccess` node surfaces with
  `ProducedSlotAccess`, returning `RuntimeProduct`.
- Renamed resolver and binding direction concepts:
  - `QueryKey::ConsumedSlot`
  - `QueryKey::ProducedSlot`
  - `BindingSource::ProducedSlot`
  - `BindingTarget::ConsumedSlot`
  - `ProductionSource::ProducedSlot`
- Removed `BindingTarget::ProducedSlot`; produced slots are producer-owned and
  should not be normal binding targets.
- Removed `PropNamespace` as a Rust-level semantic validator.
- Updated docs/tests so produced/consumed slot vocabulary is the default norm.

## Important decisions

- Direction is not part of `SlotRef`; direction belongs to the operation.
  Produced-slot access asks a node for data it produces. Consumed-slot access
  goes through the resolver because bindings, defaults, priorities, buses,
  tracing, and cycle checks all matter there.
- Buses are slot owners, not nodes. They route values but do not have node
  lifecycle or tick behavior.
- `RuntimeProduct` is the right produced payload shape for now because a
  produced slot may hold a direct runtime value or an engine-owned resource
  handle.
- For this slice, runtime slot keys may still be `ValuePath`-shaped opaque keys
  such as `config.width`. That is a compatibility shape, not the final semantic
  model.
- Bindings should be slot-level. Nested value paths are useful for reads,
  projection, and diffs, but the slot is the version boundary:
  `state.touches` has a version; `state.touches[3].id` does not.

## Future work captured

See `future.md` for the next design thread:

- Introduce structured slot values and slot-local paths.
- Decide the final `SlotValue` / `SlotPath` model.
- Keep binding endpoints at `SlotRef`, while allowing nested reads/projection
  through a separate value-reference concept.

## Validation

```bash
cargo test -p lpc-model
cargo test -p lpc-source
cargo test -p lpc-engine
cargo test -p lpc-view
cargo test -p lpc-wire
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```
