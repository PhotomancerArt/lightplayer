# SlotCodec Trait Design

## File Structure

```text
lp-core/lpc-model/src/slot_codec/
  mod.rs
  slot_codec.rs
  slot_reader.rs
  slot_writer.rs
  slot_value_codec.rs
  json_syntax_source.rs
  toml_syntax_source.rs
  syntax.rs

lp-core/lpc-slot-codegen/src/
  lib.rs

lp-core/lpc-slot-mockup/src/
  build.rs
  src/generated/slot_codec.rs
  src/tests/generated_shape_codec.rs
```

The exact writer file split can be adjusted during implementation, but the conceptual split should stay clear:

- `slot_reader.rs`: read cursors
- `slot_writer.rs`: write cursors and sink trait
- `slot_codec.rs`: `SlotCodec` trait and blanket/container impls
- `slot_value_codec.rs`: `LpValue` read/write helpers driven by `LpType`

## Architecture Summary

`SlotCodec` is the slot-native equivalent of serde’s `Serialize`/`Deserialize` pair, but intentionally smaller and more opinionated.

```text
JSON text / TOML value
        |
        v
SyntaxEventSource
        |
        v
ValueReader / ObjectReader / ArrayReader
        |
        v
T::read_slot(...)

T::write_slot(...)
        |
        v
SlotValueWriter / SlotObjectWriter / SlotArrayWriter
        |
        v
SlotWrite backend
        |
        v
JSON bytes first; other outputs later
```

The trait contract is:

```rust
pub trait SlotCodec: Sized {
    fn read_slot<S>(value: ValueReader<'_, '_, S>) -> Result<Self, SyntaxError>
    where
        S: SyntaxEventSource;

    fn write_slot<W>(
        &self,
        value: SlotValueWriter<'_, W>,
    ) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite;

    fn should_write_slot(&self) -> bool {
        true
    }
}
```

The `should_write_slot` method is field-level policy without a second trait. It lets `OptionSlot::none()` omit fields while ordinary values always write.

## Main Components

### Read Cursors

`ValueReader` is the thing `SlotCodec::read_slot` consumes. It represents exactly one incoming value. It can be lowered to an object, array, primitive, map, binary tuple, or other semantic shape.

`ObjectReader` supports:

- `next_prop()`
- `expect_discriminator(name, expected)`
- `missing_required_field(name)`

`PropReader` owns a single property value cursor and restores path state when dropped.

### Write Cursors

`SlotValueWriter` is the thing `SlotCodec::write_slot` consumes. It represents exactly one outgoing value. It can be lowered to an object, array, primitive, map, binary tuple, or other semantic shape.

JSON remains the first implemented backend, but the trait and generated code should not say `Json` unless they are explicitly constructing JSON bytes.

### Primitive And Leaf Codecs

Implement `SlotCodec` for:

- `ValueSlot<T>` where `T: SlotValue`
- `MapSlot<String, V>` where `V: SlotCodec`
- `MapSlot<u32, V>` where `V: SlotCodec`
- `OptionSlot<T>` where `T: SlotCodec`

`ValueSlot<T>` uses `T::value_shape().ty`, `ToLpValue`, and `FromLpValue`.

The helper layer should provide:

```rust
fn read_lp_value<S>(ty: &LpType, value: ValueReader<'_, '_, S>) -> Result<LpValue, SyntaxError>
where
    S: SyntaxEventSource;

fn write_lp_value<W>(
    value: SlotValueWriter<'_, W>,
    ty: &LpType,
    lp_value: &LpValue,
) -> Result<(), SlotWriteError<W::Error>>
where
    W: SlotWrite;
```

This keeps custom primitive behavior in one discoverable place instead of spreading `read_dim2u`, `write_affine2d`, and similar helpers through generated mockup code.

### Generated Record Codecs

For every discovered `SlotRecord`, generate:

```rust
impl SlotCodec for MyRecord {
    fn read_slot<S>(value: ValueReader<'_, '_, S>) -> Result<Self, SyntaxError>
    where
        S: SyntaxEventSource,
    {
        let mut out = Self::default();
        let mut object = value.object()?;
        while let Some(mut prop) = object.next_prop()? {
            match prop.name() {
                "field" => out.field = SlotCodec::read_slot(prop.value())?,
                other => return Err(prop.unknown_field(other, FIELDS)),
            }
        }
        Ok(out)
    }

    fn write_slot<W>(&self, value: SlotValueWriter<'_, W>) -> Result<(), SlotWriteError<W::Error>>
    where
        W: SlotWrite,
    {
        let mut object = value.object()?;
        if self.field.should_write_slot() {
            self.field.write_slot(object.prop("field")?)?;
        }
        object.finish()
    }
}
```

The generated code should not know how to parse `ValueSlot<u32>` or `MapSlot<String, ShaderParamDef>`. It calls `SlotCodec`.

### Enums And Discriminators

For this plan, enums remain explicit impls unless there is already enough metadata to generate them safely.

Examples:

- node definition wrapper enum with `kind`
- `MappingConfig`
- `PathSpec`
- `BindingEndpoint` with `{ ref = "..." }` / `{ value = ... }` style

These impls still use the same `SlotCodec` trait, so generated record code can treat enum fields exactly like record fields.

## Success Criteria

- The mockup compiles and passes its generated codec tests.
- `mockup_codec_policy()` is gone.
- Generated record codec impls are driven from discovered slot fields.
- Primitive/container behavior is owned by reusable `SlotCodec` impls in `lpc-model`.
- Remaining mockup-specific codec configuration is only about surface roots/discriminators/exceptions.

