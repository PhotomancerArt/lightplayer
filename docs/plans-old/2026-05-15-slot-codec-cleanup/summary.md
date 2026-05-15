# Slot Codec Cleanup Summary

Implemented.

The cleanup removes the old static/generated `SlotCodec` branch and leaves the
mockup on the registry-driven slot reader/writer path:

- deleted the generated codec renderer, mockup generated codec include, and old
  manual/generated/mock native-stream codec tests
- deleted the public `SlotCodec` trait and its hand-written impls from model
  and mockup types
- rewired mockup storage tests to use `SlotShapeRegistry::read_slot_toml`,
  `write_slot_toml_data`, and `write_slot_json_value`
- deleted the old `lpc-wire` SlotData JSON/TOML serializers
- removed the temporary `SlotJson*` compatibility aliases
- added `JsonValue::raw_json` so the engine project-read streaming envelope can
  embed registry-written slot JSON without keeping the old wire serializer

Validation run:

- `cargo test -p lpc-model`
- `cargo test -p lpc-slot-codegen`
- `cargo test -p lpc-slot-mockup`
- `cargo test -p lpc-wire`
- `cargo check -p lpc-model --no-default-features`
- `cargo check -p lpc-wire --no-default-features`

Known follow-up:

- `cargo check -p lpc-engine` currently fails before reaching this cleanup in
  `lpc-shared/src/project/builder.rs` because older builder code still passes raw
  primitives into semantic `ValueSlot<T>` constructors.
