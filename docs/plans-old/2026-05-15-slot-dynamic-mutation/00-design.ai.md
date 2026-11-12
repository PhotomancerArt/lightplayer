# Slot Dynamic Mutation Design

## Scope

Add a generic mutable slot access layer and use it as the foundation for runtime mutations and later default-and-mutate deserialization.

## File Structure

```text
lp-core/
  lpc-model/
    src/
      slot/
        mod.rs
        slot_access.rs
        slot_mut_access.rs
        slot_accessor.rs
        slot_mutation.rs
        value_slot.rs
      lib.rs

  lpc-slot-macros/
    src/
      attr.rs
      record.rs

  lpc-slot-mockup/
    src/
      engine/
        runtime.rs
      source/
        mapping.rs
        node_def.rs
      tests/
        mutation.rs
        generated_shape_codec.rs
```

## Architecture Summary

The mutable slot layer mirrors the existing read-only slot layer.

Read side today:

```text
SlotAccess
  -> SlotDataAccess
    -> SlotRecordAccess::field(index)
    -> SlotValueAccess::value()
```

Mutation side:

```text
SlotMutAccess
  -> SlotDataMutAccess
    -> SlotRecordMutAccess::field_mut(index)
    -> SlotValueMutAccess::set_lp_value(...)
```

The generic mutation engine owns path walking and validation. The generated/derived code owns only typed field dispatch.

## Main Components

### `SlotMutAccess`

Mutable counterpart to `SlotAccess`:

```rust
pub trait SlotMutAccess: SlotAccess {
    fn data_mut(&mut self) -> SlotDataMutAccess<'_>;
}
```

Static record types that derive `SlotRecord` should implement this automatically.

### `SlotDataMutAccess`

Mutable counterpart to `SlotDataAccess`:

```rust
pub enum SlotDataMutAccess<'a> {
    Unit(&'a mut Revision),
    Value(&'a mut dyn SlotValueMutAccess),
    Record(&'a mut dyn SlotRecordMutAccess),
    Map(&'a mut dyn MapSlotMutAccess),
    Enum(&'a mut dyn SlotEnumMutAccess),
    Option(&'a mut dyn SlotOptionMutAccess),
}
```

The exact `Unit` representation may change if unit revisions should remain private, but the important thing is that enum unit payloads can expose variant revision for conflict checks.

### `SlotValueMutAccess`

Leaf mutation by `LpValue`:

```rust
pub trait SlotValueMutAccess {
    fn changed_at(&self) -> Revision;
    fn set_lp_value(
        &mut self,
        revision: Revision,
        value: LpValue,
    ) -> Result<(), SlotMutationError>;
}
```

`ValueSlot<T>` implements this where `T: SlotValue`.

### `SlotRecordMutAccess`

Generated/derived field dispatch:

```rust
pub trait SlotRecordMutAccess {
    fn fields_revision(&self) -> Revision {
        Revision::default()
    }

    fn field_mut(&mut self, index: usize) -> Option<SlotDataMutAccess<'_>>;
}
```

`#[derive(SlotRecord)]` should generate the mutable mirror of the existing immutable field dispatch.

### `MapSlotMutAccess`

First pass should support access to existing entries only:

```rust
pub trait MapSlotMutAccess {
    fn keys_revision(&self) -> Revision;
    fn get_mut(&mut self, key: &SlotMapKey) -> Option<SlotDataMutAccess<'_>>;
}
```

Insert/remove can come later as explicit operations.

### `SlotOptionMutAccess`

First pass should support mutating existing `some` payloads:

```rust
pub trait SlotOptionMutAccess {
    fn presence_revision(&self) -> Revision;
    fn data_mut(&mut self) -> Option<SlotDataMutAccess<'_>>;
}
```

Creating `some` from `none` should be a later explicit operation.

### `SlotEnumMutAccess`

Enums are the trickiest case. Split active payload mutation from variant switching:

```rust
pub trait SlotEnumMutAccess {
    fn variant_revision(&self) -> Revision;
    fn variant(&self) -> &str;
    fn data_mut(&mut self) -> SlotDataMutAccess<'_>;
}
```

This supports mutating fields inside the active variant. It does not switch variants.

Variant switching is an explicit default-construction operation:

```rust
pub trait SlotEnumDefaultVariant: SlotEnumMutAccess {
    fn set_variant_default(
        &mut self,
        revision: Revision,
        variant: &str,
    ) -> Result<(), SlotMutationError>;
}
```

All slot enum variants are expected to have defaults at the model layer. Logic
that needs a fully authored or renderable model validates that separately.

JSON/TOML enum deserialization should:

1. Read the discriminator.
2. Call `set_variant_default`.
3. Mutate fields inside the active payload.

The convenience form `set_variant_from_slot_data` can be added later.

### `SlotMutation`

Generic mutation should live in `lpc-model`, not the mockup runtime:

```rust
pub fn set_slot_value(
    root: &mut dyn SlotMutAccess,
    registry: &SlotShapeRegistry,
    path: &SlotPath,
    revision: Revision,
    value: LpValue,
) -> Result<(), SlotMutationError>
```

It should:

1. Compile or walk the path against `SlotShapeRegistry`.
2. Check the root shape id.
3. Walk mutable slot data by field index, map key, option `some`, and active enum payload.
4. Require the final shape to be a value leaf.
5. Check `LpValue` compatibility through the target leaf's `SlotValue` conversion.
6. Set the leaf with the provided revision.

## Enum Path Behavior

Enum paths should behave like paths through the active variant payload.

Example shape:

```text
mapping: Enum {
  square: Record { origin, size }
  path_points: Record { paths, sample_diameter }
}
```

If active variant is `square`, this should work:

```text
mapping.origin
```

This should fail with a clear error:

```text
mapping.sample_diameter
```

because `sample_diameter` is not in the active variant.

Discriminator/variant switching should not be inferred from field names. It
should be an explicit mutation operation, and for now that operation constructs
the requested variant from defaults.
