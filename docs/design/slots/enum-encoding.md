# Slot Enum Encoding

Slot enums have one runtime data model: an active variant plus that variant's
payload. Encoding controls only how authored documents represent that enum.

## Tagged Encoding

Tagged encoding is the default for slot enums. The active variant is stored in
a discriminator field, currently `kind`, and record or unit payload data is
flattened beside it.

```toml
kind = "PathPoints"
sample_diameter = 0.5
```

This remains the default because existing node artifacts and project files use
it widely. Types that do not opt into another encoding keep this shape.

## External Encoding

External encoding stores the active variant as the single property of the enum
object. The property's value is the payload.

Scalar payload:

```toml
file = "compute.glsl"
```

Structured payload:

```toml
[point]
x = 10
y = 11
```

Unit payload:

```toml
[disabled]
```

In Rust, opt in with `#[slot(enum_encoding = "external")]`:

```rust
#[derive(Slotted)]
#[slot(enum_encoding = "external", rename_all = "snake_case")]
enum ShaderSourceSpec {
    #[default]
    File(SourcePathSlot),
    Inline(ValueSlot<String>),
}
```

Variant property names are chosen in this order:

1. `#[slot(name = "...")]` on the variant.
2. `#[slot(rename_all = "snake_case")]` on the enum.
3. The Rust variant name.

For example:

```rust
#[derive(Slotted)]
#[slot(enum_encoding = "external", rename_all = "snake_case")]
enum Thing {
    #[default]
    OptionA(ValueSlot<u32>),
    #[slot(name = "custom")]
    OptionB(ValueSlot<u32>),
}
```

uses authored names `option_a` and `custom`.

## Validation

The external enum object must contain exactly one variant property. Zero
properties, multiple properties, and unknown variant names are errors.

The variant payload is decoded with the same slot shape machinery as any other
slot. External encoding therefore supports value, record, unit, map, option,
and referenced payload shapes.

## Future Field-Key Encoding

Field-presence discrimination is intentionally separate. A future encoding may
allow a unique `#[slot(key)]` field inside each record variant to select the
active variant while still keeping other fields at the same table level.

That design is useful for extensible config namespaces, but it needs distinct
shape metadata and ambiguity checks, so it is not part of external encoding.
