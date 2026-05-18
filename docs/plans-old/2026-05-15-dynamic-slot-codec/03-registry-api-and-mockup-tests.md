# Phase 03: Registry API And Mockup Tests

## Scope Of Phase

Expose ergonomic registry read APIs and prove them against the mockup project
shapes.

In scope:

- Add registry methods in
  `lp-core/lpc-model/src/slot/slot_shape_registry.rs`:
  - `read_slot_json`
  - `read_slot_toml`
  - `read_slot_from`
- Add or update exports as needed.
- Add mockup tests that read real mockup objects from JSON and TOML through the
  registry APIs.
- Cover records, maps, enums, options, dynamic shapes, and resource/product
  leaves if practical.

Out of scope:

- Replacing existing generated `SlotCodec` tests.
- Dynamic writing.
- Filesystem loading.

## Code Organization Reminders

- Keep registry methods small delegating wrappers.
- Put substantial logic in `slot_codec/dynamic_slot_reader.rs`.
- Add mockup tests in a new focused file if no existing file is an exact fit,
  for example:
  `lp-core/lpc-slot-mockup/src/tests/dynamic_slot_codec.rs`.

## Sub-Agent Reminders

- Do not commit.
- Do not hide failures by weakening existing generated codec tests.
- Do not introduce mockup-specific special cases into `lpc-model`.
- Report any deviations, especially if a shape is not creatable and needs an
  explicit factory registration fix.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`
- `lp-core/lpc-model/src/slot_codec/mod.rs`
- `lp-core/lpc-slot-mockup/src/tests/mod.rs`
- `lp-core/lpc-slot-mockup/src/tests/shape_factory.rs`
- New optional file:
  `lp-core/lpc-slot-mockup/src/tests/dynamic_slot_codec.rs`

Expected registry API shape:

```rust
impl SlotShapeRegistry {
    pub fn read_slot_json(&self, shape_id: SlotShapeId, json: &str)
        -> Result<Box<dyn SlotMutAccess>, SyntaxError>;

    pub fn read_slot_toml(&self, shape_id: SlotShapeId, value: &toml::Value)
        -> Result<Box<dyn SlotMutAccess>, SyntaxError>;

    pub fn read_slot_from<S>(&self, shape_id: SlotShapeId, source: S)
        -> Result<Box<dyn SlotMutAccess>, SyntaxError>
    where
        S: SyntaxEventSource;
}
```

Mockup tests should prove:

- Reading `ProjectDef` JSON/TOML produces a slotted object with `nodes` entries.
- Reading `FixtureDef` can switch `mapping` by discriminator and fill payload
  fields.
- Reading omitted fields leaves empty defaults.
- Unknown fields error.
- Invalid enum discriminators report expected values.
- Reading an explicitly dynamic shader-node shape works after registering its
  dynamic shape.

Use slot access assertions where downcasting is unavailable. Do not add
type-specific escape hatches just for tests.

## Validate

```bash
cargo fmt -p lpc-model -p lpc-slot-mockup --check
cargo test -p lpc-model dynamic_slot_reader
cargo test -p lpc-slot-mockup dynamic_slot_codec
cargo test -p lpc-slot-mockup shape_factory
```
