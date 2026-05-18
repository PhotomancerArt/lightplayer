# `LpValue::Enum` Plan Summary

Implementation complete.

This plan added atomic enum values to `LpValue`, taught SlotCodec to read/write
them, and moved `BindingEndpoint` onto the new representation.

Built:

- `LpValue::Enum { variant: u32, payload: Option<Box<LpValue>> }`
- `LpType::Enum { name: Option<String>, variants: Vec<ModelEnumVariant> }`
- `LpType::Any` for dynamically typed definition-time payloads, used by
  `BindingEndpoint::Literal(LpValue)`
- JSON and TOML value codec support for enum values using authored `kind` names
  and indexed in-memory variants
- dynamic mutation type checks for `LpType::Any` and `LpType::Enum`
- `BindingEndpoint` conversion through enum-backed `LpValue`

Main decisions:

- use `LpValue::Enum` for atomic semantic choices inside `ValueSlot<T>`
- use `SlotShape::Enum` for addressable structured variants
- store enum variants by index in `LpValue`; keep names in `LpType`
- keep `LpType::Enum.name` optional debug/schema context, not identity
- treat `LpValue::Struct` field-name storage as older/mixed design that can be
  revisited separately

Validation:

- `cargo test -p lpc-model`
- `cargo test -p lpc-slot-mockup -- --test-threads=1`
- `cargo check -p lpc-engine`

Known follow-up:

- `cargo test -p lpc-slot-mockup` without `--test-threads=1` can race on the
  global ambient revision used by tests. The focused failing mutation test
  passes alone and the full mockup suite passes single-threaded.
