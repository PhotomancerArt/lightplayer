# Dynamic Slot Codec Notes

## Scope Of Work

Build the first generic dynamic read path for slot-modeled data:

- Given a `SlotShapeRegistry`, a `SlotShapeId`, and a syntax reader/source,
  create a default object through the registry and apply the incoming syntax to
  that object by walking slot shapes.
- Expose ergonomic registry entry points for TOML values, JSON text, and JSON
  event streams.
- Keep validation separate. Reading produces shaped sentinel data when fields
  are omitted; recursive validation is a later layer.
- Teach `read_lp_value`/`write_lp_value` enough custom resource/product support
  that dynamic reading can cover all current `LpType` leaves instead of
  excluding `Resource` and `Product`.

Out of scope:

- Generic dynamic writing.
- Recursive validation.
- Schema versioning or unknown-field compatibility.
- Replacing generated/static `SlotCodec` readers.
- Product/resource syntax bikeshedding beyond a minimal explicit shape that can
  round-trip current values.

## Current State

- `SlotShapeRegistry::create_default(shape_id)` exists and returns
  `Box<dyn SlotMutAccess>` for static and dynamic creatable shapes.
- `SlotFactory` supports static, dynamic, and unsupported creation behavior.
- Defaults are now intentionally empty sentinel data. `Unset` exists for
  enums without an honest semantic default, currently `ResourceDomain` and
  `BindingEndpoint`.
- Generic mutation helpers exist:
  - `set_slot_value`
  - `set_slot_variant_default`
  - `insert_slot_map_entry_default`
  - `set_slot_option_some_default`
- Those helpers operate by `SlotPath`, which is useful but awkward for a
  streaming reader. The dynamic reader should probably walk `SlotDataMutAccess`
  and `SlotShape` directly, adding tiny local helpers as needed rather than
  formatting paths for every nested value.
- `slot_codec` already has:
  - `JsonSyntaxSource` for JSON text streams.
  - `TomlSyntaxSource` for already-parsed `toml::Value`.
  - `SlotReader`, `ValueReader`, `ObjectReader`, and `ArrayReader`.
  - `read_lp_value`/`write_lp_value` for most primitive `LpType` leaves.
- `read_lp_value` currently rejects `LpType::Resource` and
  `LpType::Product(_)`. This is a self-imposed gap. Custom codec support for
  `ResourceRef`, `VisualProduct`, `ControlProduct`, and `ProductRef` is fine
  for now.

## User Notes

- The desired public surface should be registry-centered, roughly
  `registry.read_*` or `registry.load_*`.
- It should be very easy to test reading objects from TOML, JSON text, and JSON
  streams.
- Validation is separate after reading. The reader should not reject empty
  sentinel values merely because they are not meaningful to the engine.
- Resource/product `LpValue` support should be implemented now; it is not hard
  enough to justify excluding them from the dynamic codec.

## Proposed Answers To Open Questions

### Registry Method Naming

- **Question:** Should the public API use `read_*` or `load_*`?
- **Suggested answer:** Use `read_*` for syntax-to-object APIs in
  `lpc-model`. Reserve `load_*` for filesystem/domain loading layers.
- **Proposed methods:**
  - `read_slot_json(shape_id, json: &str)`
  - `read_slot_toml(shape_id, value: &toml::Value)`
  - `read_slot_from(shape_id, source: impl SyntaxEventSource)`
  - Internal/public-low-level `read_slot_value(shape_id, value: ValueReader)`

### Where The Dynamic Walker Lives

- **Question:** Should `apply_reader_to_slot` live in `slot` or `slot_codec`?
- **Suggested answer:** Put the syntax-driven implementation in
  `lpc-model/src/slot_codec/dynamic_slot_reader.rs`, because it consumes
  `ValueReader` and syntax sources. Keep registry convenience wrappers in
  `SlotShapeRegistry` as small delegating methods.

### Reader Strategy

- **Question:** Should the dynamic reader use existing path-based mutation
  helpers or direct mutable access?
- **Suggested answer:** Use direct `(SlotDataMutAccess, SlotShape)` recursion.
  This avoids allocating/formatting paths during streaming and keeps error paths
  tied to the existing `SlotReader` path tracking.

### Resource/Product Syntax

- **Question:** What syntax should `read_lp_value` use for resources/products?
- **Suggested answer:** Use explicit object forms for now:

```toml
# ResourceRef
domain = "runtime_buffer" # or "unset"
id = 7

# ProductRef
kind = "visual"
node = 2
output = 0

kind = "control"
node = 3
output = 0
[preferred_extent]
rows = 1
samples_per_row = 12
```

This is clear, testable, and can evolve later if product/ref string syntax
changes.

### Unknown And Missing Fields

- **Question:** Should missing fields be errors?
- **Suggested answer:** Missing fields keep the default sentinel value. Unknown
  fields are errors until schema versioning exists.

### Options

- **Question:** How should option slots read dynamically?
- **Suggested answer:** If an option field/property exists, create `Some` with
  a default payload and read into it. If the property is absent, keep `None`.
  A literal `null` can be supported for JSON by leaving/setting `None` if the
  reader can cheaply detect it; if that is awkward, defer explicit null support
  and document absent-property as the supported `None` form for M1.
