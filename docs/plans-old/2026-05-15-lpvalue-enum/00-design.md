# `LpValue::Enum` Design

## Scope

Add atomic enum values to the LightPlayer value language and teach the slot
codec to read/write them.

This plan does not remove serde from `lpc-model`; it prepares the value model so
that later migration can make semantic enum leaves slot-native without forcing
them into strings or slot-structured enums.

## File Structure

```text
lp-core/lpc-model/src/
  value/
    lp_value.rs          # add LpValue::Enum
    lp_type.rs           # add LpType::Enum and ModelEnumVariant
  slot_codec/
    slot_value_codec.rs  # JSON/read/write helpers for enum values
    dynamic_slot_writer.rs # TOML writer for enum values
  binding/
    binding_endpoint.rs  # first semantic user after base support

docs/design/slots/
  overview.md
  values.md
  serialization.md
```

## Architecture Summary

`LpValue::Enum` represents an atomic enum-like value. It is a leaf payload, not
a slot subtree.

`LpType::Enum` describes the allowed variants and optional payload type for each
variant. The slot codec uses this type context to validate reads/writes.

`ValueSlot<T>` remains the revision boundary for semantic leaves. Semantic enum
types such as `BindingEndpoint` convert to/from `LpValue::Enum` through
`ToLpValue` and `FromLpValue`.

`SlotShape::Enum` remains the structured slot enum. It is used when clients need
paths into variant payload fields or separate mutation/sync behavior inside the
active variant.

## Main Components

### `LpValue::Enum`

Shape:

```rust
Enum {
    variant: u32,
    payload: Option<Box<LpValue>>,
}
```

`variant` is an index into `LpType::Enum.variants`. `payload: None` represents
a unit variant.

### `LpType::Enum`

Shape:

```rust
Enum {
    name: Option<String>,
    variants: Vec<ModelEnumVariant>,
}
```

Each variant has a name and an optional payload type.

### Slot Codec

JSON/TOML syntax should be explicit and friendly first. Compact special forms
can come later.

Initial syntax:

```toml
kind = "Value"
payload = 0.75
```

or, as an inline value:

```toml
endpoint = { kind = "Value", payload = 0.75 }
```

Unit variants omit payload:

```toml
endpoint = { kind = "Unset" }
```

The reader validates that the variant exists and that payload presence/type
matches `LpType::Enum`.

The in-memory value stores the variant index, not the variant name:

```rust
LpValue::Enum {
    variant: 1,
    payload: Some(Box::new(LpValue::F32(0.75))),
}
```

This follows the broader payload rule: names live in shapes/types; values carry
compact payloads.

### Binding Endpoint

After base support, `BindingEndpoint` should use `LpValue::Enum` instead of
debug/string encoding.

Suggested variant mapping:

- `Unset` -> unit variant
- `Bus` -> string payload
- `Node` -> string payload
- `Literal` -> dynamic payload

Use `LpType::Any` for the `Literal` payload. This keeps the storage layer honest
about the dynamic payload while leaving semantic validation to the surrounding
binding/model logic.
