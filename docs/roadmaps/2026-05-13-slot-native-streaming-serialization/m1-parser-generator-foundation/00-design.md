# M1 Design: Parser And Generator Foundation

## Shape

M1 introduces a small `lpc_wire::slot::native` module:

- `SyntaxEvent`: shape-agnostic syntax tokens.
- `SyntaxEventSource`: pull-based event source trait.
- `JsonSyntaxSource`: direct JSON text parser that emits syntax events.
- `TomlSyntaxSource`: adapter from `toml::Value` to syntax events.
- `SyntaxNode`: temporary reference/tree representation built from events.
- `SlotReader`: typed helper API over a `SyntaxNode` plus `SlotShapeRegistry`.
- `SlotJsonWriter`: output helper over the existing `JsonWriter`.

The temporary tree is deliberately not the long-term embedded ideal. It lets us
stabilize the event vocabulary and reader ergonomics first. Later phases can
make generated code consume events directly when needed.

## Reader Example

```rust
ManualConfig {
    brightness: reader.prop("brightness")?.f32()?,
    pin: reader.prop("pin")?.u32()?,
    mapping: reader.prop("mapping")?.string()?,
}
```

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

