# `LpValue::Enum` Implementation Notes

## Scope

Implement atomic enum support in `LpValue` so semantic slot leaves can represent
enum-like choices without becoming slot-structured enums.

This is a prerequisite for removing serde from `lpc-model` because some model
definition values, especially binding endpoints, are atomic semantic choices
that do not fit cleanly into scalar/string-only `LpValue` storage.

## Current State

`LpValue` lives in:

- `lp-core/lpc-model/src/value/lp_value.rs`

It currently supports:

- scalars
- vectors/matrices
- arrays/lists
- structs
- resources
- products

`LpType` lives in:

- `lp-core/lpc-model/src/value/lp_type.rs`

It currently mirrors the structural storage variants, with typed forms for
arrays, lists, structs, resources, and products.

Slot value read/write logic lives in:

- `lp-core/lpc-model/src/slot_codec/slot_value_codec.rs`
- `lp-core/lpc-model/src/slot_codec/dynamic_slot_writer.rs`

The reader path already reads values by `LpType`; the writer path writes values
by matching `(LpType, LpValue)`.

`BindingEndpoint` currently lives in:

- `lp-core/lpc-model/src/binding/binding_endpoint.rs`

It is already a semantic `SlotValue`, but it currently represents itself as
`LpValue::String`. That works for refs and unset, but does not honestly
round-trip `BindingEndpoint::Literal(LpValue)`.

## Design Decision

Add atomic enum support to `LpValue`.

Rule:

- `LpValue::Enum` is for enum-like choices that behave as one value inside a
  `ValueSlot<T>`.
- `SlotShape::Enum` is for addressable slot structure whose active variant has
  mutable fields, payload revisions, and slot paths.

Use `LpValue::Enum` when:

- one `ValueSlot<T>` owns the revision
- the whole choice changes as a unit
- callers do not address fields inside the payload through slot paths
- payload data is another `LpValue` or absent for unit variants

Use `SlotShape::Enum` when:

- changing a subfield of the active variant should be a separate slot mutation
- the variant payload has slot metadata or nested slot structure
- clients/tools need paths into the active payload

## Proposed Shape

Add:

```rust
pub enum LpValue {
    // existing variants...
    Enum {
        variant: u32,
        payload: Option<Box<LpValue>>,
    },
}
```

Add:

```rust
pub enum LpType {
    // existing variants...
    Enum {
        name: Option<String>,
        variants: Vec<ModelEnumVariant>,
    },
}

pub struct ModelEnumVariant {
    pub name: String,
    pub payload: Option<LpType>,
}
```

The `name` is optional for the same reason `LpType::Struct` has an optional
name: it preserves semantic debugging/schema context without forcing all dynamic
uses to invent a stable global type name.

Payload principle:

- names live in `LpType` and slot metadata
- `LpValue` carries compact payload data
- authored JSON/TOML can use names, and SlotCodec maps them through `LpType`
- `LpValue::Enum.variant` is an index into `LpType::Enum.variants`
- `LpValue::Array` remains shared by fixed arrays and lists because `LpType`
  distinguishes the container kind
- `LpValue::Struct` currently stores field names, but this is now considered an
  older/mixed design choice; a future cleanup may make struct payloads indexed
  too

## Binding Endpoint Direction

Once enum value support exists, `BindingEndpoint` should move toward indexed
enum storage:

- variant `0`, `Unset` -> no payload
- variant `1`, `Bus` -> string payload
- variant `2`, `Node` -> string payload
- variant `3`, `Literal` -> value payload

The exact authored compact syntax for binding endpoints can be implemented after
the base enum value support is in place.

## Docs Updated

The approved boundary is documented in:

- `docs/design/slots/overview.md`
- `docs/design/slots/values.md`
- `docs/design/slots/serialization.md`

## Open Questions

### Q1. Should `BindingEndpoint` use variants `Bus` / `Node` or one `Ref` variant?

Context: Rust currently distinguishes `Bus(BusSlotRef)` and `Node(NodeSlotRef)`.
Both serialize as ref strings in authored syntax.

Suggested answer: keep `Bus` and `Node` as separate `LpType::Enum` variants for
lossless semantic round trip. Compact authored syntax can still use
`{ ref = "..." }` and infer the concrete Rust variant from the string.

### Q2. Should `LpValue::Enum` payload be optional or unit-as-empty-object?

Context: unit variants like `Unset` should be first-class and compact.

Suggested answer: use `Option<Box<LpValue>>`; `None` is the unit payload.
