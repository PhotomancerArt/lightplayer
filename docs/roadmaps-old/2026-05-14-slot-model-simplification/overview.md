# Slot Model Simplification Roadmap

## Motivation

The slot system is trying too hard in a few places. This is not Serde; it is a
small data modeling system for LightPlayer objects that need to be reflected,
serialized, deserialized, synced, edited, and patched.

The cleanup should make the simple path obvious:

```rust
#[derive(SlotRecord)]
pub struct OutputDef {
    pub pin: ValueSlot<u32>,
}
```

From that, the system should be able to derive shape, view, sync, and codec
machinery without a second hidden schema.

## Architecture

Keep the current crate layout for now. The cleanup happens in place:

```text
lpc-model/src/slot/
  core shape/access/path/data model

lpc-model/src/slot_codec/
  generic syntax reader/writer and LpValue helpers

lpc-model/src/slots/
  semantic value definitions and shape/editor metadata

lpc-slot-codegen/src/
  discovered record model
  shape generation
  view generation
  codec generation

lpc-slot-mockup/src/
  proof that slot-authored data records produce the desired codec behavior
```

## Main Decisions

- `ValueSlot<T>` is the generic revision-tracked leaf container.
- `T: SlotValue + ToLpValue + FromLpValue` owns semantic shape and conversion.
- Most leaf serialization should be generic `LpValue` conversion.
- Slot records are intentionally simple public data records.
- No `#[slot(skip)]` in the generated record path.
- Complex objects either delegate to a slot-data field or implement custom
  machinery manually.
- No `lpc-slot` or `lpc-domain` extraction in this roadmap.

## Risks

- Leaf cleanup may expose ambiguous semantic wrappers that should remain
  domain-specific for now.
- Removing skipped/private fields may require touching mockup/domain shapes.
- Codec generation can grow too much code if helpers are not shared.
- The old static codec table may be tempting to preserve; it should be treated
  as temporary scaffolding.
