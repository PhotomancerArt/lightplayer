# External Slot Enums Design

## Scope of Work

Add opt-in externally tagged enum encoding to the slot system.

This plan changes only the slot model, slot codec, derive macro, tests, and documentation needed for external enum syntax. It does not migrate shader source definitions or implement field-presence enum discrimination.

## File Structure

```text
lp-core/lpc-model/src/slot/
  slot_shape.rs
  slot_shape_builder.rs

lp-core/lpc-model/src/slot_codec/
  dynamic_slot_reader.rs
  dynamic_slot_writer.rs

lp-core/lpc-slot-macros/src/
  attr.rs
  slotted_enum.rs

lp-core/lpc-model/tests/
  slotted_enum_derive.rs

docs/design/slots/
  enum-encoding.md
```

## Architecture Summary

The existing slot data model remains unchanged. `EnumSlot<T>` continues to store one active variant and payload; `SlotShape::Enum` continues to describe the available variants.

The new behavior is an authored encoding choice on `SlotShape::Enum`:

```rust
pub enum SlotEnumEncoding {
    Tagged { field: SlotName },
    External,
}
```

Existing enums default to:

```rust
SlotEnumEncoding::Tagged { field: "kind" }
```

Externally tagged enums opt in with a derive attribute:

```rust
#[derive(Slotted)]
#[slot(enum_encoding = "external", rename_all = "snake_case")]
enum ShaderSourceSpec {
    #[default]
    File(SourcePathSlot),
    Inline(ValueSlot<String>),
}
```

Authored TOML:

```toml
[glsl]
file = "compute.glsl"
```

Structured variant TOML:

```toml
[thing.a]
x = 10
y = 10
```

## Main Components and Interactions

### Slot Shape Metadata

`SlotShape::Enum` gains an encoding field:

```rust
Enum {
    meta: SlotMeta,
    encoding: SlotEnumEncoding,
    variants: Vec<SlotVariantShape>,
}
```

The encoding field must default during serde decode so previously serialized shape metadata remains compatible.

### Shape Builder

`slot::shape::variant(...)` stays unchanged.

Add an enum builder helper for external encoding, or add a helper that takes an encoding:

```rust
shape::external_enum(vec![...])
```

Existing `shape::enum` / direct enum construction should keep producing tagged `kind` enums.

### Dynamic Slot Reader

`read_enum` dispatches by encoding:

- `Tagged { field }`
  - Existing behavior, except the discriminator field name comes from shape metadata rather than hard-coded `"kind"`.
- `External`
  - Read the enum value as an object with exactly one property.
  - Property name selects the variant.
  - Property value is decoded against the selected variant's payload shape.
  - Error on zero properties, multiple properties, or unknown variant property.

External payload decoding should support value, record, unit, map, option, and reference shapes by delegating to `apply_reader_to_slot`.

### Dynamic Slot Writer

TOML and JSON writers dispatch by enum encoding:

- `Tagged { field }`
  - Existing behavior, with discriminator field name from shape metadata.
- `External`
  - Emit an object with one property named after the active variant.
  - Emit the active variant payload as that property's value.

This means external record payloads write as nested objects and external value payloads write as scalar values.

### Derive Macro

`#[derive(Slotted)]` for enums gains:

```rust
#[slot(enum_encoding = "external")]
#[slot(rename_all = "snake_case")]
```

Variant-level `#[slot(name = "...")]` remains the strongest override.

`rename_all = "snake_case"` should apply only to variants without explicit `#[slot(name = "...")]`.

At minimum, support `snake_case`; other policies can be added later if needed.

### Documentation

Add project docs explaining:

- Default tagged enum syntax.
- External enum syntax.
- How payload shapes map to authored TOML.
- How `#[slot(enum_encoding = "external")]`, `#[slot(rename_all = "snake_case")]`, and `#[slot(name = "...")]` interact.
- Why field-presence discrimination is separate future work.

Add Rust docs to the new encoding type and relevant builder helpers.

## Compatibility

Existing enum syntax remains the default:

```toml
kind = "PathPoints"
sample_diameter = 0.5
```

No existing node artifact should change syntax or parsed data because no existing enum opts into external encoding during this plan.

## Validation Strategy

Targeted model and macro tests should cover:

- Existing tagged enum decode/write still works.
- External newtype/value variant decode/write.
- External record variant decode/write.
- External unit variant decode/write.
- External enum rejects zero properties.
- External enum rejects multiple properties.
- External enum rejects unknown variant property.
- `rename_all = "snake_case"` maps `OptionA` to `option_a`.
- Explicit `#[slot(name = "...")]` overrides `rename_all`.

Final validation should include:

```bash
cargo test -p lpc-model slot_codec --lib
cargo test -p lpc-model --features derive --test slotted_enum_derive
cargo test -p lpc-model
cargo test -p lpc-slot-macros
cargo check -p lpc-engine
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```
