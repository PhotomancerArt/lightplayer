# Slot Codec Writers Notes

## Scope

Build the generic writer half of the new slot codec system before removing old
codec code. The target is shape-driven serialization from `SlotAccess` /
`SlotDataAccess`, using the slot registry as the source of truth.

This plan covers:

- JSON writer API for dynamic/static slot objects
- TOML writer API for dynamic/static slot objects
- registry-level write entry points symmetric with current read entry points
- mockup tests proving round trips through the generic reader/writer path
- enough replacement coverage to unblock deletion of old `SlotCodec` generated
  code and old `lpc-wire` `SlotData` serializers in the following cleanup

This plan does not cover:

- deleting old codec code
- adopting the new writer in real project loading/messages beyond minimal
  compile-preserving prep
- schema versioning or validation

## User Notes

- "writers first"
- We need both JSON and TOML writers eventually, so do both now.
- The previous cleanup plan found that deleting old serializers is blocked by
  writer coverage and APIs.
- Keep the slot system as the source of truth.
- JSON should stay stream-friendly for embedded memory pressure.
- TOML-authored objects are usually small, and TOML already has table layout
  constraints that make a value tree acceptable.

## Current State

### Dynamic Reader Exists

- `lpc-model/src/slot/slot_shape_registry.rs`
  - `read_slot_json(shape_id, json)`
  - `read_slot_toml(shape_id, &toml::Value)`
  - `read_slot_from(shape_id, source)`
- `lpc-model/src/slot_codec/dynamic_slot_reader.rs`
  - `read_dynamic_slot(registry, shape_id, value)`
  - `apply_reader_to_slot(data, shape, registry, value)`
- static objects can be recovered from registry reads through `Any` downcasting.

### Low-Level JSON Writer Exists

- `lpc-model/src/slot_codec/slot_writer.rs`
  - `SlotWriter<W>`
  - `SlotObjectWriter`
  - `SlotArrayWriter`
  - `SlotValueWriter`
  - `SlotWrite`
- It streams JSON directly to a byte sink.
- It still has compatibility aliases from the first JSON-only prototype:
  - `SlotJsonWriter`
  - `SlotJsonValue`
  - `SlotJsonWrite`
  - `SlotJsonWriterError`
  - `SlotJsonObject`
  - `SlotJsonArray`

### Leaf Value Writing Exists

- `lpc-model/src/slot_codec/slot_value_codec.rs`
  - `write_lp_value(value_writer, ty, value)`
  - `write_untyped_lp_value(value_writer, value)`
- It already knows resource/product leaves.
- The TOML encoder in `lpc-wire/src/slot/authored_toml.rs` has its own
  separate `encode_lp_value`; that duplication should be replaced by a new
  `toml` value writer path in `lpc-model`.

### Old Generic SlotData Writers Exist

- `lpc-wire/src/slot/authored_toml.rs`
  - `encode_slot_data_access_toml(shape, data, registry)`
  - `encode_slot_data_toml(shape, data, registry)`
- `lpc-wire/src/slot/slot_data_json.rs`
  - `write_slot_data_json(writer, shape_id, data, registry)`
- These walk `SlotShape` + `SlotDataAccess` directly.
- They are conceptually close to the desired writer, but live in `lpc-wire`,
  duplicate value encoding, and write `SlotData` rather than `SlotAccess`.

### Callers Still Depending On Old Writers

- `lpc-slot-mockup/src/tests/storage_codec.rs`
  - old TOML encode/decode tests
  - old direct JSON slot data test
- `lpc-engine/src/engine/project_read_stream.rs`
  - uses `write_slot_data_json` for project read responses

## Proposed Writer Shape

Add a new shape-driven writer beside the dynamic reader:

- `lpc-model/src/slot_codec/dynamic_slot_writer.rs`
  - `write_dynamic_slot_json(writer, registry, root)`
  - `write_dynamic_slot_json_value(value_writer, registry, shape_id, data)`
  - `write_dynamic_slot_toml(registry, root) -> Result<toml::Value, SlotWriteErrorLike>`
  - lower-level helpers that walk `(SlotShape, SlotDataAccess)`

Registry APIs:

- `SlotShapeRegistry::write_slot_json<W>(&self, root: &dyn SlotAccess, out: W)`
- `SlotShapeRegistry::write_slot_json_value<W>(&self, id, data, value_writer)`
  - useful for replacing old `write_slot_data_json`
- `SlotShapeRegistry::write_slot_toml(&self, root: &dyn SlotAccess) -> Result<toml::Value, _>`
- `SlotShapeRegistry::write_slot_toml_data(&self, id, data) -> Result<toml::Value, _>`
  - useful for replacing old `encode_slot_data_access_toml`

The JSON writer should stream directly. The TOML writer should return
`toml::Value` because TOML table rendering is not naturally streaming and disk
authored TOML is expected to be small.

## TOML vs JSON Policy

Use the same semantic shape for both formats:

- records write fields by slot field name
- maps write as object/table keyed by authored map key text
- enums write `"kind"` first conceptually
  - JSON: object with `kind` prop, then payload props
  - TOML: table with `kind = "..."`
- options:
  - `None` fields are omitted from enclosing records
  - writing a root option that is `None` should serialize as an empty TOML
    table and JSON `null` or error; see open question
- unit enum payloads write only `kind`
- value leaves use typed `write_lp_value` semantics for JSON
- TOML gets equivalent typed value conversion in `lpc-model`, not `lpc-wire`

## Open Questions

### Q1. What should root-level `None` serialize to?

Context: record fields can simply omit `None`. Root-level options and map values
cannot be omitted by their parent if the writer was asked to write exactly that
slot.

Suggested answer:

- JSON root `None`: write `null`
- TOML root `None`: write an empty table
- record field `None`: omit

This keeps record output clean and gives root-level calls a defined behavior.

### Q2. Should empty records/maps write or omit?

Context: the old `SlotCodec::should_write_slot` had per-type emission policy.
The slot-shaped generic writer needs a simple universal policy.

Suggested answer:

- write all present record fields except `None`
- JSON may omit empty records/maps when that is cheap and local
- TOML should write empty records/maps when the caller explicitly writes that
  object
- omit `None` option fields only

This matches the user's preference to save JSON bandwidth if it is not hard,
without making the writer policy elaborate.

User answer: omit empty things in JSON for bandwidth if it is easy, but do not
over-invest. The rest of the proposed decisions make sense.

### Q3. Should TOML writer preserve field order?

Context: `toml` has `display` and model's `std` feature uses `toml/preserve_order`.
Current shapes have field order and enum discriminator policy.

Suggested answer:

- rely on `toml::Table` insertion order where supported
- insert `kind` first for enums
- then insert payload fields in slot-shape order

If no preserve-order is available in a no-std build, the semantic value is still
correct even if display order changes.

### Q4. Should the writer include a root discriminator automatically?

Context: `ProjectDef` by itself does not have a `kind` field. Wrapper enums like
`NodeDef` provide a discriminator. Existing authored TOML examples include root
`kind` for node defs.

Suggested answer:

- generic writer does not invent a root discriminator for record shapes
- discriminators belong to slot enum shapes
- tests that need root `kind` should write the enum wrapper shape, once generic
  enum-wrapper machinery exists

This keeps the writer purely shape-driven and avoids hidden type lists.

### Q5. What error type should dynamic writers use?

Context: `SlotWriteError<W::Error>` is currently JSON-writer oriented. TOML
building has no sink write error, but can fail on shape/data mismatch.

Suggested answer:

- introduce `SlotCodecError` or `SlotDataWriteError` for shape/data semantic
  errors
- keep `SlotWriteError<W::Error>` for sink errors
- JSON dynamic writer returns `Result<(), SlotWriteError<W::Error>>` and maps
  semantic mismatch to `SlotWriteError::Serialize` initially, or extend
  `SlotWriteError` with `InvalidSlotData(String)`
- TOML dynamic writer returns the semantic error directly

I lean toward extending/renaming the writer error now, because "Serialize" is
too opaque for the generic shape writer.
