# M1 Design: Parser And Generator Foundation

## Shape

M1 introduces a small `lpc_wire::slot::native` module:

- `SyntaxEvent`: shape-agnostic syntax tokens.
- `SyntaxEventSource`: pull-based event source trait.
- `JsonSyntaxSource`: direct JSON text parser that emits syntax events.
- `TomlSyntaxSource`: adapter from `toml::Value` to syntax events.
- `SlotReader`: streaming typed helper API over a `SyntaxEventSource` plus
  `SlotShapeRegistry`.
- `SlotJsonWriter`: output helper over the existing `JsonWriter`.

The reader must not require a temporary syntax tree. JSON events should be
parsed on demand, and manual/generated typed construction should consume the
stream directly.

## Reader Example

```rust
let mut object = reader.object()?;
while let Some(prop) = object.next_prop()? {
    match prop.name() {
        "brightness" => brightness = Some(prop.value().f32()?),
        "pin" => pin = Some(prop.value().u32()?),
        other => return Err(prop.unknown_field(other, &["brightness", "pin"])),
    }
}
```

Discriminator-first enums use `reader.expect_discriminator("kind")?` after the
object start has been consumed.

## Writer Example

```rust
let mut object = writer.object()?;
object.prop("brightness")?.f32(config.brightness)?;
object.prop("pin")?.u32(config.pin)?;
object.prop("mapping")?.string(&config.mapping)?;
object.finish()?;
```

## Validation

- `cargo test -p lpc-wire slot::native`
- `cargo test -p lpc-slot-mockup native_stream`
