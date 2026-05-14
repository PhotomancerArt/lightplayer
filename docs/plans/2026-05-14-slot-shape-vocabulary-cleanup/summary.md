# Slot Shape Vocabulary Cleanup Summary

## Result

The cleanup landed the "shape, not root" direction in code and docs.

- `SlotShapeRegistry` now exposes shape-based APIs:
  - `register_shape*`
  - `ensure_shape*`
  - `replace_shape*`
  - `unregister_shape*`
- `SlotRecord` now derives `StaticSlotShape`, `SlotAccess`, and
  `StaticSlotAccess` for every named-field record.
- Build-time shape discovery now registers every `SlotRecord`, rather than
  only records marked with `#[slot(root)]`.
- `#[slot(root)]` was removed from the model and mockup. Rust derive helper
  attributes cannot be used as bare `#[slot]`, and the empty marker is no
  longer needed anyway.
- Codegen vocabulary now uses registered shape / codec type terminology:
  - `StaticSlotRoot` -> `StaticRegisteredShape`
  - `SlotCodecRoot` -> `SlotCodecType`
  - mock codec module `.roots` -> `.types`
- `SlotAccess` remains the generic runtime slot object trait.
- `root` remains only where it means "the root of this path traversal" or
  actual synced runtime roots in view/wire/mockup layers.

## Important Design Notes

- Slot-annotated records are shape-bearing by default. Usage sites decide
  whether a value is persisted, synced, path-addressed, or used as a message
  payload.
- The registry is a shape registry, not a root registry.
- There is no separate top-level "slot root" concept in the static model right
  now. Runtime systems may still have roots as containers or path anchors.
- The codegen surface is still intentionally small and shared-helper oriented,
  because binary size remains one of the major motivations for SlotCodec.

## Validation

- `cargo fmt`
- `cargo test -p lpc-slot-codegen`
- `cargo test -p lpc-model slot_shape_registry`
- `cargo test -p lpc-model slot_accessor`
- `cargo test -p lpc-model slot_lookup`
- `cargo test -p lpc-model --test slot_record_derive`
- `cargo test -p lpc-model --test slot_accessor`
- `cargo test -p lpc-slot-mockup shape_codegen`
- `cargo test -p lpc-slot-mockup generated_shape_codec`
- `cargo test -p lpc-slot-mockup`
- `cargo check -p lpc-model --no-default-features`
- `cargo check -p lpc-wire --no-default-features`
- `cargo check -p lpc-slot-mockup`
