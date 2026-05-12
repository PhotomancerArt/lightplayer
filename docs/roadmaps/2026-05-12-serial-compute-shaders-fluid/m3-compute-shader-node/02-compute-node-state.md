# Phase 2: Compute Node State And Materialization

## Scope

- Add `ComputeShaderNode`.
- Add dynamic runtime state shape/data helpers for produced compute slots.
- Convert value outputs and sentinel-array map outputs into `SlotData`.
- Resolve consumed value slots through `TickContext`.

## Implementation Notes

- `lp-shader` returns raw `LpsValueF32`; the node materializes slot data.
- Consumed map slots should return a clear unsupported error.
- Produced map slots should support `u32` keys and sentinel mappings only.
- The node's runtime state root should be instance-specific.

## Validation

- Add unit tests for:
  - shape registration of produced value/map slots
  - sentinel array to `SlotMapDyn`
  - resolving a produced map through `QueryKey::ProducedSlot`
- Run `cargo test -p lpc-engine compute -- --nocapture`
