# Slot Model Simplification Notes

## Scope

Organize the slot/codec cleanup into a coherent roadmap. The goal is to make
the slot system simple enough that generated serialization feels obvious:

- slot records are plain public data objects
- `ValueSlot<T>` owns revision-tracked leaf storage
- `T: SlotValue + ToLpValue + FromLpValue` owns semantic shape metadata and
  conversion
- record shape/view/codec generation is driven from `#[derive(SlotRecord)]`
  source
- custom serialization exists only as explicit primitive/semantic escape
  hatches, not hidden record schemas

This roadmap does not split `lpc-slot` or `lpc-domain` out of `lpc-model`.
Those crate boundaries may still be attractive later, but the current effort
should stay focused on making the mockup and core model coherent in place.

## Current State

### What Feels Wrong

The current system has several overlapping abstractions:

- `ValueSlot<T>` already implements generic revision-tracked leaf storage.
- `SlotValue`, `ToLpValue`, and `FromLpValue` already describe semantic value
  shape and conversion.
- Many files under `lpc-model/src/slots` define custom `FooSlot` structs that
  duplicate `ValueSlot<T>` storage, revision, serde, `SlotValueAccess`, and
  `FieldSlot` behavior just to attach shape/editor metadata.
- The generated SlotCodec prototype still has `mockup_source_codec_module()`,
  a static table that manually describes domain records and fields.

This makes the system feel harder than it should be. It looks like each leaf
needs a codec and each record needs a schema table, even though the existing
slot traits should already contain most of that information.

### Existing Good Pieces

- `ValueSlot<T>` is already the generic revision-tracked leaf container.
- `ValueSlot<T>` implements `SlotValueAccess` for `T: ToLpValue`.
- `ValueSlot<T>` implements `FieldSlot` for `T: SlotValue`.
- `SlotValue` already provides `value_shape()`.
- `ToLpValue` and `FromLpValue` already provide semantic conversion.
- Several semantic values are already modeled correctly as values:
  - `Dim2u`
  - `Affine2d`
  - `ColorOrderValue`
  - `ResourceRef`
  - `VisualProduct`
  - `ControlProduct`
  - shader option enums
- Shape/view generation already discovers `#[derive(SlotRecord)]` records.

### Existing Problem Pieces

#### Leaf Wrappers Duplicate Generic Storage

Examples:

- `RatioSlot`
- `PositiveF32Slot`
- `RenderOrderSlot`
- `XySlot`
- `SourcePathSlot`
- `ArtifactPathSlot`
- `RelativeNodeRefSlot`
- `ResourceRefSlot`
- `ColorOrderSlot`
- `Dim2uSlot`
- `Affine2dSlot`

Some of these wrap a typed semantic value, while others directly wrap storage
primitives. Many repeat the same pattern:

- `inner: WithRevision<T>`
- `new`
- `with_version`
- `set`
- `revision` / `changed_revision`
- `value`
- `SlotValueAccess`
- `Serialize`
- `Deserialize`
- `FieldSlot`
- `*_shape()`

For many of these, the simpler model should be:

```rust
pub type RatioSlot = ValueSlot<Ratio>;
```

where `Ratio` is the semantic value that owns:

- `SlotValue::value_shape()`
- `ToLpValue`
- `FromLpValue`
- validation, if needed

#### Primitive Semantics Are Missing Names

Some wrapper types exist because there is no semantic value type. For example,
`RatioSlot` stores `f32` directly, but "ratio" is the semantic concept. A
cleanup likely needs a small newtype:

```rust
pub struct Ratio(pub f32);
pub type RatioSlot = ValueSlot<Ratio>;
```

Likewise, `PositiveF32Slot` may become:

```rust
pub struct PositiveF32(pub f32);
pub type PositiveF32Slot = ValueSlot<PositiveF32>;
```

This preserves metadata and validation without duplicating `ValueSlot`.

#### Some Slots May Just Be Aliases

Some wrappers may not need semantic value newtypes yet:

- `SourcePathSlot` may become `ValueSlot<String>` or `StringSlot`.
- `ArtifactPathSlot` may become `ValueSlot<String>` or a real semantic
  `ArtifactPath` if validation/behavior is important.

The rule should be: use a semantic value type only when the semantics matter.
Otherwise, use a plain value slot alias.

#### SlotRecord Is Still Too Permissive For The Goal

The simplified generated path should require simple public slot data fields.
Private fields and skipped fields make codegen complicated and muddy the data
model.

Desired rule:

- slot fields are public
- no `#[slot(skip)]` in generated records
- discriminator fields such as `kind` are not data fields
- complicated models delegate to a public slot-data field or implement custom
  machinery manually

#### SlotCodec Prototype Has A Shadow Schema

`lpc-slot-codegen/src/lib.rs` still has `mockup_source_codec_module()`, which
manually lists records and fields. This table must be removed after the leaf
and record model rules are simplified.

## User Notes

- "Simple simple simple. We're trying way too hard here. This is not Serde."
- Slot records model basic data objects that can be serialized,
  deserialized, synced, reflected, edited, etc.
- It is fine to force models to be simple.
- There should be an escape hatch for fully custom behavior:
  - put slot data in a field and delegate to it
  - write a fully custom implementation
- Do not split `lpc-slot` or `lpc-domain` right now. The concepts are
  interlinked and the split would become its own project.
- Focus on building the mockup. Do not worry about validating the whole
  workspace while the model is moving.
- Most "codecs" should just be `LpValue` conversion, not custom per-leaf
  serialization logic.
- Custom code is acceptable when it is primitive-focused, explicit, and easy
  to find.

## Suggested Simplified Architecture

### Leaf Values

```rust
pub trait SlotValue: ToLpValue + FromLpValue {
    const SHAPE_ID: SlotShapeId;
    fn value_shape() -> SlotValueShape;
}

pub struct ValueSlot<T> {
    inner: WithRevision<T>,
}
```

`ValueSlot<T>` is the normal leaf container. `T` owns semantic metadata and
conversion.

Convenience aliases are fine:

```rust
pub type StringSlot = ValueSlot<String>;
pub type U32Slot = ValueSlot<u32>;
pub type RatioSlot = ValueSlot<Ratio>;
pub type Dim2uSlot = ValueSlot<Dim2u>;
```

### Record Values

```rust
#[derive(SlotRecord)]
pub struct OutputDef {
    pub pin: ValueSlot<u32>,
    pub options: OptionSlot<OutputDriverOptionsConfig>,
}
```

Generated machinery targets this simple shape.

### Custom Escape Hatches

Complex runtime/domain objects use one of:

```rust
pub struct ComplexThing {
    pub slots: ComplexThingSlots,
    cache: RuntimeCache,
}

#[derive(SlotRecord)]
pub struct ComplexThingSlots {
    pub brightness: RatioSlot,
}
```

or a fully custom implementation of the relevant slot/codec traits.

## Open Questions

### Q1. What should `#[slot(skip)]` become?

Suggested answer: Remove it from the generated SlotRecord path. If there is a
future need, add `#[slot(transient)]` with clear disk/wire semantics. For now,
every field in a slot record is a slot field.

### Q2. Should simple semantic wrappers use newtypes or aliases?

Suggested answer: Use aliases when raw storage is enough (`StringSlot`,
`U32Slot`). Use semantic newtypes when the value has editor metadata,
validation, or a named shape (`Ratio`, `PositiveF32`, `Dim2u`, `Affine2d`).

### Q3. Should we convert all leaf wrappers before SlotCodec codegen?

Suggested answer: No. Convert a representative set first:

- `RatioSlot`
- `PositiveF32Slot`
- `RenderOrderSlot`
- maybe `Dim2uSlot`

Then proceed to record/codegen cleanup once the pattern is proven.

### Q4. Where should generic syntax-to-`LpValue` helpers live?

Suggested answer: `lpc-model/src/slot_codec`. These helpers read/write
`LpValue` according to `SlotValueShape` / `LpType`. They should not be named
after domain records.

### Q5. How much should this roadmap validate?

Suggested answer: keep validation focused:

- `cargo fmt`
- `cargo test -p lpc-model <targeted tests>`
- `cargo test -p lpc-slot-codegen`
- `cargo test -p lpc-slot-mockup`
- `cargo check -p lpc-model --no-default-features`
- `cargo check -p lpc-wire --no-default-features`

Broader engine/firmware validation happens after production adoption begins.
