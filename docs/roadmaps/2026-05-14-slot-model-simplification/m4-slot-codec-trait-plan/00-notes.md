# SlotCodec Trait Plan Notes

## Scope

Build the serde-inspired slot-native codec layer that the mockup has been circling:

- define a real `SlotCodec` trait in `lpc-model`
- define the read/write cursor names and responsibilities clearly
- implement codec behavior on primitive slot containers and value leaves
- generate record codec impls from discovered `#[derive(SlotRecord)]` structs
- remove `mockup_codec_policy()` as a shadow field schema
- keep the mockup as the proof target before adopting this in the real domain loading/message paths

The immediate deliverable is not full production adoption. It is a clean mockup where authored types use slots, field types own serialization behavior, and the generated code just stitches the fields together.

## Current State

`lpc-model/src/slot_codec` already has the low-level streaming foundation:

- `SyntaxEventSource` emits shape-agnostic syntax events.
- `JsonSyntaxSource` streams JSON text into events.
- `TomlSyntaxSource` adapts a `toml::Value` into events.
- `SlotReader`, `ValueReader`, `ObjectReader`, and `ArrayReader` are semantic read cursors over those events.
- `SyntaxError` tracks message, path, and optional source span.
- `SlotJsonWriter`, `SlotJsonValue`, `SlotJsonObject`, and `SlotJsonArray` are the current JSON writer prototype.

The read side is close to the desired shape. The write side works but is named as JSON-specific, so it does not yet communicate the intended format-neutral codec contract.

`lpc-model/src/slot/value_slot.rs` already has the main slot containers:

- `ValueSlot<T>`
- `MapSlot<K, V>`
- `OptionSlot<T>`

`ValueSlot<T>` is now the intended standard leaf container. Semantic leaf values own `SlotValue`, `ToLpValue`, and `FromLpValue`.

`lpc-slot-codegen` now discovers slot records and fields through `discover_static_slot_records`, but generated mockup codecs still rely on `mockup_codec_policy()`. That policy table duplicates record fields, constructors, default expressions, read expressions, and write expressions. It is the main remaining smell.

## User Decisions

- This should copy the parts of serde that make sense: types own serialization behavior, derive/codegen stitches fields together.
- This is not meant to be a general-purpose serde replacement. It is for the slot system and can be opinionated.
- The important interface is:

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
  }
  ```

- `ValueReader` reads one value from the event stream.
- `SlotValueWriter` writes one value to the output stream.
- A `SlotCodec` implementation must consume or emit exactly one value.
- No in-memory tree should be introduced as the main path.
- `mockup_codec_policy()` should disappear because field behavior lives on field types.
- Remaining policy should be surface-level only: top-level codec functions, discriminators, and explicit transient/omitted fields.

## Open Questions

### Writer abstraction naming

Current code uses `SlotJsonValue`, `SlotJsonObject`, `SlotJsonArray`, `SlotJsonWrite`, and `SlotJsonWriterError`.

Suggested answer: introduce format-neutral names:

- `SlotWrite`
- `SlotWriteError`
- `SlotWriter`
- `SlotValueWriter`
- `SlotObjectWriter`
- `SlotArrayWriter`

Keep JSON as the first concrete backend. A compatibility type alias is acceptable during migration if it keeps the patch smaller, but the public trait should use the neutral names.

### Should `OptionSlot<T>` write `null` or omit?

Suggested answer: `SlotCodec for OptionSlot<T>` reads/writes a present value. Record codegen handles field omission by asking the field whether it should be written. `None` should normally omit the property for authored disk/wire shapes. We do not need `null` unless a later protocol explicitly asks for it.

This likely means `SlotCodec` needs either:

- a small `fn should_write_slot(&self) -> bool { true }`, or
- a sibling `SlotFieldCodec` trait for field-level policy.

Suggested first pass: keep one trait and add `should_write_slot`, because it is simpler and directly supports `OptionSlot`.

### How should missing fields work?

Suggested answer: generated record readers start from `Default::default()` when the record implements `Default`, then mutate fields that appear in the stream. This matches the user’s preferred model that defaults are generally leaf/container-level and available through Rust’s `Default`.

For records without `Default`, codegen can require every field or fail codegen for now. The mockup should prefer adding `Default` where the authored shape wants defaults.

### How much policy remains?

Suggested answer: policy may name codec surfaces and discriminated wrappers, but it must not list every record field or every per-field read/write expression. If there is an omitted/transient field before the slot metadata supports that, list only that exception by type/field name and keep it discoverable.

