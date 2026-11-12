# Streaming Reader Correction Design

## Scope Of Work

Replace the tree-backed M1 reader with a genuinely stream-backed reader for
JSON and a compatible TOML adapter. Remove `SyntaxNode` entirely.

This plan corrects the foundation before M2 codegen starts, so generated code
targets the right model from the beginning.

Out of scope:

- Proc-macro/codegen implementation.
- Production loader/message adoption.
- Full schema versioning.
- Fully streaming TOML parsing from source text.

## File Structure

Target shape:

```text
lp-core/lpc-wire/src/slot/
  mod.rs
  native/
    mod.rs
    syntax_event.rs
    syntax_error.rs
    json_syntax_source.rs
    toml_syntax_source.rs
    streaming_slot_reader.rs
    slot_json_writer.rs

lp-core/lpc-slot-mockup/src/tests/
  native_stream.rs
```

If the implementation stays small enough, the file split can be reduced, but
the concepts should remain clear:

- syntax events and spans
- parser/adapters
- streaming semantic reader
- writer facade
- tests

## Architecture Summary

```text
JSON text
    -> JsonSyntaxSource::next_event()
    -> StreamingSlotReader
    -> manual/generated typed construction

toml::Value
    -> TomlSyntaxSource::next_event()
    -> StreamingSlotReader
    -> same typed construction shape
```

`JsonSyntaxSource` must parse events on demand. It must not prebuild
`Vec<SyntaxEvent>`.

`TomlSyntaxSource` may be backed by borrowed `toml::Value` state because TOML
authored artifacts are small, but it should expose the same event source
interface.

`StreamingSlotReader` owns or borrows a syntax source and consumes tokens as
typed construction asks for them. It tracks semantic path independently of
source span.

## Main Components And Interactions

### `SyntaxEvent`

Events remain shape-agnostic:

```rust
StartObject
Prop { name, span }
EndObject
StartArray
EndArray
StringChunk { text, is_last, span }
Number { text, span }
Bool { value, span }
Null { span }
```

Exact enum field layout can differ, but events should carry optional source
span when available.

### `JsonSyntaxSource`

`JsonSyntaxSource::next_event()` parses only enough input to return the next
event. It keeps a small stack for object/array state and source offsets for
span reporting.

String values may allocate a chunk string in M1, but the source must not
allocate the whole document or an event vector.

### `TomlSyntaxSource`

`TomlSyntaxSource` adapts a `toml::Value` tree into the same event interface.
It can use a traversal stack over borrowed TOML values. It does not need spans
unless they are cheaply available.

### `StreamingSlotReader`

The reader provides semantic helpers:

- `object() -> ObjectReader`
- `array() -> ArrayReader`
- `expect_discriminator(name) -> ValueReader`
- scalar reads: `string`, `f32`, `u32`, `bool`
- `binary_base64_tuple`
- `skip_value`
- diagnostic constructors for unknown fields, invalid discriminators, expected
  types, and expected delimiters

Normal records use scanner-style code:

```rust
let mut object = reader.object()?;
while let Some(prop) = object.next_prop()? {
    match prop.name() {
        "brightness" => brightness = Some(prop.value().f32()?),
        other => return Err(prop.unknown_field(other, &["brightness"])),
    }
}
object.finish()?;
```

Enums/wrappers use discriminator-first code:

```rust
let kind = reader.expect_discriminator("kind")?.string()?;
match kind {
    "TextureDef" => TextureDef::deserialize(reader),
    "OutputDef" => OutputDef::deserialize(reader),
    other => Err(reader.invalid_discriminator_value(
        "kind",
        other,
        &["TextureDef", "OutputDef"],
    )),
}
```

`expect_discriminator` reads the next property and its value. It must leave the
reader inside the same object so the selected variant can consume remaining
fields.

### Diagnostics

Errors should include:

- category or future-stable code
- path
- optional source span
- actual value where useful
- expected value or expected values where available

Example:

```text
Invalid discriminator `kind` at mapping.kind: "Blark12".
Expected one of: TextureDef, OutputDef, FixtureDef.
```

### `SyntaxNode`

Remove it. Do not keep a reference/debug tree in this corrective plan.
