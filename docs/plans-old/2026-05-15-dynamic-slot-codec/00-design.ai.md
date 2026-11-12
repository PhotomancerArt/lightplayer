# Dynamic Slot Codec Design

## Scope Of Work

Implement the first generic dynamic read path for slot-modeled data:

- Registry-centered `read_*` APIs for TOML values, JSON text, and arbitrary
  syntax event sources.
- A shape-driven reader that creates a default slot object and applies incoming
  syntax by walking `SlotShape` plus `SlotDataMutAccess`.
- Custom `LpValue` resource/product read/write support so all current `LpType`
  leaves can participate.
- Focused tests in `lpc-model` and `lpc-slot-mockup` proving basic objects,
  maps, enums, options, resources/products, TOML, JSON text, and stream sources.

Validation is explicitly out of scope. The dynamic reader may produce
shaped-but-invalid sentinel data such as empty strings, zero ids, absent
options, empty maps, or `Unset` enum variants.

## File Structure

```text
lp-core/lpc-model/src/
  slot/
    slot_shape_registry.rs
      # public registry read_* convenience methods

  slot_codec/
    mod.rs
      # exports the dynamic reader APIs
    dynamic_slot_reader.rs
      # read_dynamic_slot and apply_reader_to_slot implementation
    slot_value_codec.rs
      # resource/product LpValue read/write support
```

## Public API

The registry owns the ergonomic entry points:

```rust
impl SlotShapeRegistry {
    pub fn read_slot_json(
        &self,
        shape_id: SlotShapeId,
        json: &str,
    ) -> Result<Box<dyn SlotMutAccess>, SyntaxError>;

    pub fn read_slot_toml(
        &self,
        shape_id: SlotShapeId,
        value: &toml::Value,
    ) -> Result<Box<dyn SlotMutAccess>, SyntaxError>;

    pub fn read_slot_from<S>(
        &self,
        shape_id: SlotShapeId,
        source: S,
    ) -> Result<Box<dyn SlotMutAccess>, SyntaxError>
    where
        S: SyntaxEventSource;
}
```

The lower-level slot-codec module exposes the implementation helper for tests
and future generated code:

```rust
pub fn read_dynamic_slot<S>(
    registry: &SlotShapeRegistry,
    shape_id: SlotShapeId,
    value: ValueReader<'_, '_, S>,
) -> Result<Box<dyn SlotMutAccess>, SyntaxError>
where
    S: SyntaxEventSource;
```

If useful during implementation, `apply_reader_to_slot` may be public within
the crate or exported from `slot_codec`, but it should remain a low-level
building block rather than the primary user-facing API.

## Architecture Summary

The dynamic reader is the runtime analogue of generated `SlotCodec` code:

1. Resolve the requested `SlotShapeId` in the registry.
2. Call `registry.create_default(shape_id)` to allocate a mutable object.
3. Recursively walk `(SlotShape, SlotDataMutAccess, ValueReader)`.
4. Mutate only the fields/properties present in the syntax stream.
5. Leave omitted fields at their default sentinel values.
6. Error on unknown fields, invalid discriminators, wrong syntax shapes, or
   unsupported factories.

The walker should prefer direct mutable access rather than path-string mutation:

- `Record`: read an object, look up each property in the record fields, recurse
  into that field.
- `Map`: read an object, parse each property name as the map key, insert a
  default entry, then recurse into that entry.
- `Enum`: read an object, require the first property to be `kind`, switch the
  active variant with a default payload, then recurse into the active payload.
- `Option`: when syntax for the option is present, create `Some(default)` and
  recurse into the payload. Omitted properties stay `None`.
- `Value`: read an `LpValue` using the declared `LpType`, then set it through
  `SlotValueMutAccess`.
- `Ref`: resolve the referenced shape and continue.
- `Unit`: accept an empty object or null-like representation only if the
  existing reader support makes this simple; otherwise keep the initial
  implementation focused on records/maps/enums/options/values and document
  unit limitations in tests.

## Resource/Product Value Syntax

`slot_value_codec.rs` should support explicit object forms:

```toml
# ResourceRef
domain = "runtime_buffer" # or "unset"
id = 7

# Visual product
kind = "visual"
node = 2
output = 0

# Control product
kind = "control"
node = 3
output = 0

[preferred_extent]
rows = 1
samples_per_row = 12
```

JSON uses the equivalent object forms.

These forms are intentionally direct and custom. They do not need to mirror
Serde exactly; they only need to be clear, stable enough for tests, and easy to
replace later if product/ref string syntax changes.

## Error Policy

- Unknown fields are errors.
- Invalid enum discriminators report the invalid value and expected values.
- Missing fields keep default sentinel data.
- Unsupported factories produce a clear read error.
- Validation of sentinel values is not part of reading.
