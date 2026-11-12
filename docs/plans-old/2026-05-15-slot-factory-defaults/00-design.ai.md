# Slot Factory Defaults Design

## Scope

Add default object creation to the slot registry so generic readers can create
slot objects from shape ids.

Primary API:

```rust
impl SlotShapeRegistry {
    pub fn create_default(
        &self,
        id: SlotShapeId,
    ) -> Result<Box<dyn SlotMutAccess>, SlotFactoryError>;
}
```

## File Structure

```text
lp-core/
  lpc-model/
    src/
      slot/
        mod.rs
        slot_access.rs
        slot_data.rs
        slot_factory.rs
        slot_mut_access.rs
        slot_shape_registry.rs
        slot_mutation.rs
        value_slot.rs

  lpc-slot-codegen/
    src/
      render/
        slot_shapes.rs

  lpc-slot-mockup/
    src/
      engine/
        runtime.rs
      tests/
        shape_factory.rs
        mutation.rs
```

## Architecture Summary

The registry becomes the bridge between shape metadata and default slot objects.

```text
SlotShapeRegistry
  shape id -> SlotShapeEntry
                  shape metadata
                  runtime factory hook

create_default(shape_id)
  static entry  -> Box::new(T::default()) as Box<dyn SlotMutAccess>
  dynamic entry -> Box::new(DynamicSlotObject { shape_id, data })
  unsupported   -> clear SlotFactoryError
```

Static and dynamic defaults both return the same erased object type:

```rust
Box<dyn SlotMutAccess>
```

Generic readers can then parse into any created object through the existing
mutation/access layer.

Factories are required creation behavior for registered root shapes. Dynamic
construction is one possible factory, not an implicit fallback for all shape
metadata. Some registered shapes may intentionally be uncreatable as standalone
objects; those should have an explicit unsupported factory so failure is clear
and deliberate.

Creation is also the opt-in point for generic mutation/deserialization. A
generic reader may mutate a caller-provided `&mut dyn SlotMutAccess`, or it may
ask the registry to create one. If `create_default` returns an unsupported
factory error, deserialization fails at that shape boundary.

## Main Components

### `SlotFactory`

Add a factory module in `lpc-model/src/slot/slot_factory.rs`.

Suggested surface:

```rust
pub type SlotFactoryFn =
    fn(&SlotShapeRegistry, SlotShapeId) -> Result<Box<dyn SlotMutAccess>, SlotFactoryError>;

#[derive(Clone, Copy)]
pub enum SlotFactory {
    Static(SlotFactoryFn),
    Dynamic,
    Unsupported,
}

impl SlotFactory {
    pub const fn for_default<T>() -> Self
    where
        T: SlotMutAccess + Default + 'static;
    pub const fn dynamic() -> Self;
    pub const fn unsupported() -> Self;

    pub fn create_default(
        self,
        registry: &SlotShapeRegistry,
        id: SlotShapeId,
    ) -> Result<Box<dyn SlotMutAccess>, SlotFactoryError>;
}
```

The factory enum keeps the creation policy explicit while avoiding a second
boxed trait object. Static factories carry a function pointer; dynamic and
unsupported factories are zero-capture enum variants.

### `SlotFactoryError`

Add a focused error type:

```rust
pub enum SlotFactoryError {
    MissingShape(SlotShapeId),
    MissingReferencedShape(SlotShapeId),
    UnsupportedFactory(SlotShapeId),
    EmptyEnum(SlotShapeId),
    InvalidShape { message: String },
}
```

Keep errors friendly enough for codec tests, but avoid over-designing spans or
path reporting in this milestone.

### `DynamicSlotObject`

Add an owned dynamic root object:

```rust
pub struct DynamicSlotObject {
    shape_id: SlotShapeId,
    data: SlotData,
}
```

It implements:

```rust
impl SlotAccess for DynamicSlotObject
impl SlotMutAccess for DynamicSlotObject
```

It should expose accessors for tests/debugging:

```rust
pub fn new(shape_id: SlotShapeId, data: SlotData) -> Self;
pub fn into_data(self) -> SlotData;
pub fn data_ref(&self) -> &SlotData;
```

### Dynamic Data Builder

The dynamic factory internally builds `SlotData` from `SlotShape`:

```rust
fn default_data_for_shape(
    registry: &SlotShapeRegistry,
    root_id: SlotShapeId,
    shape: &SlotShape,
) -> Result<SlotData, SlotFactoryError>
```

Rules:

- `Ref`: resolve through registry and build referenced shape.
- `Unit`: `SlotData::Unit { revision: current_revision() }`
- `Value`: `SlotData::Value(WithRevision::new(current_revision(), default_lp_value(ty)))`
- `Record`: build each field's data in shape order.
- `Map`: empty `SlotMapDyn` with current revision.
- `Option`: `SlotOptionDyn::none_with_version(current_revision())`
- `Enum`: first variant with default payload and current revision; error for empty variants.

`default_lp_value(&LpType)` should cover all current `LpType` cases.

### Registry Integration

Add factory-aware registration without making snapshots serialize factories.

Preferred implementation:

```rust
pub struct SlotShapeRegistry {
    pub ids_revision: Revision,
    shapes: BTreeMap<SlotShapeId, SlotShapeEntry>,
    factories: BTreeMap<SlotShapeId, SlotFactory>,
}
```

All registration paths should choose a factory explicitly. To keep call sites
readable, prefer named helpers:

- `register_static_shape_with_factory(...)`
- `register_dynamic_shape(...)`
- `register_uncreatable_shape(...)`
- matching `ensure_*` and `replace_*` helpers where needed.

The existing `register_shape`/`ensure_shape` methods can remain as compatibility
wrappers during migration, but the implementation should audit current call
sites and move them to explicit static/dynamic/uncreatable registration where
the intent is known.

New methods:

```rust
register_shape_with_factory(...)
register_shape_named_with_factory(...)
ensure_shape_with_factory(...)
ensure_shape_named_with_factory(...)
replace_shape_with_factory(...)
replace_shape_named_with_factory(...)
create_default(id)
```

`snapshot()` keeps only shape entries. `apply_snapshot()` must restore complete
factory behavior with a documented snapshot policy. Because factory choice is
the creation opt-in, restored snapshot entries should use explicit unsupported
factories unless local code later installs static or dynamic factories.

### Static Shape Registration

Codegen should register factories for static shapes:

```rust
registry.ensure_shape_named_with_factory(
    T::SHAPE_ID,
    T::shape_name().unwrap_or(...),
    T::slot_shape(),
    SlotFactory::for_default::<T>(),
)
```

The helper can be:

```rust
impl SlotFactory {
    pub const fn for_default<T>() -> Self
    where
        T: SlotMutAccess + Default + 'static;
}
```

If Rust const generic bounds make that awkward, use generated monomorphic
factory functions:

```rust
fn create_project_def_default(
    _: &SlotShapeRegistry,
    _: SlotShapeId,
) -> Result<Box<dyn SlotMutAccess>, SlotFactoryError> {
    Ok(Box::new(crate::nodes::project::ProjectDef::default()))
}
```

Prefer the smallest generated code that compiles cleanly.

### Typed Map/Option/Enum Creation Hooks

The public registry factory solves root creation. Containers also need local
creation hooks for deserialization.

Add explicit methods without changing `set_slot_value` policy:

```rust
trait MapSlotMutAccess {
    fn insert_default(
        &mut self,
        revision: Revision,
        key: &SlotMapKey,
        registry: &SlotShapeRegistry,
        value_shape: &SlotShape,
    ) -> Result<(), SlotMutationError>;
}

trait SlotOptionMutAccess {
    fn set_some_default(
        &mut self,
        revision: Revision,
        registry: &SlotShapeRegistry,
        some_shape: &SlotShape,
    ) -> Result<(), SlotMutationError>;
}
```

Typed containers can use `V::default()`. Dynamic containers can use the dynamic
data builder internally.

Enum switching already exists through `SlotEnumDefaultVariant`; dynamic
`SlotEnum` can be upgraded to construct default payloads through the registry
and variant shape.

### Codec Bridge Shape

This milestone does not need to replace all generated codec code, but it should
prove the intended API shape with tests:

```rust
let mut obj = registry.create_default(ProjectDef::SHAPE_ID)?;
reader_or_test_helper_applies_fields(obj.as_mut(), &registry)?;
```

If `create_default` fails because the shape is explicitly unsupported,
deserialization should return that as a normal data/model error:

```text
cannot deserialize shape 0x12345678: shape is not creatable
```

For map construction, add a test that starts from a default object with an empty
map and uses an explicit generic insertion helper before setting a nested leaf.

## Phase Overview

1. Add factory object types and dynamic root object.
2. Integrate runtime-only factories into the registry.
3. Implement dynamic default data creation from shapes.
4. Teach static codegen/registration to install typed default factories.
5. Add map/option/enum creation hooks for default-and-mutate reads.
6. Prove the mockup vertical slice.
7. Cleanup and validation.
