# ValueSlot / SlotValue Model Design

## Scope

This design makes slot leaves crisp:

- `ValueSlot<T>` is storage and revision tracking.
- `T: SlotValue` is semantic payload, conversion, shape, and editor metadata.
- Most `FooSlot` structs become aliases to `ValueSlot<Foo>`.
- `#[derive(SlotValue)]` generates the boring semantic value impls.
- Slot value ids derive from Rust type names by default.

## File Structure

```text
lp-core/
  lpc-model/
    src/
      slot/
        slot_value.rs
        value_slot.rs
        mod.rs
      slots/
        ratio.rs
        positive_f32.rs
        render_order.rs
        source_path.rs
        artifact_path.rs
        xy.rs
        dim2u.rs
        affine2d.rs
        color_order.rs
        relative_node_ref.rs
        resource_ref.rs
        ...
  lpc-slot-macros/
    src/
      lib.rs
      record.rs
      value.rs
      attr.rs
  lpc-slot-codegen/
    src/
      lib.rs
  lpc-slot-mockup/
    src/
      ...
```

## Architecture Summary

The model has three layers:

```text
SlotRecord
  owns addressable fields
  fields implement FieldSlot

ValueSlot<T>
  owns one revision-tracked leaf value
  implements FieldSlot when T: SlotValue

T: SlotValue
  owns semantic value shape
  converts to/from LpValue
  provides editor metadata
```

## Main Components

### `ValueSlot<T>`

`ValueSlot<T>` remains a struct:

```rust
pub struct ValueSlot<T> {
    inner: WithRevision<T>,
}
```

It owns:

- `new`
- `with_version`
- `set`
- `set_with_version`
- `revision`
- `value`
- `Default` when `T: Default`
- serde passthrough when serde remains needed
- slot access impls

It does not own semantic metadata.

### `WithRevision<T>`

`WithRevision<T>` should stay a struct because it stores real data:

```rust
pub struct WithRevision<T> {
    value: T,
    changed_at: Revision,
}
```

Renaming is allowed if it makes the model clearer, but it is not the first priority. If renamed, `Revisioned<T>` is the preferred candidate.

### `SlotValue`

`SlotValue` is the semantic leaf contract:

```rust
pub trait SlotValue: Sized + ToLpValue + FromLpValue {
    const SHAPE_ID: SlotShapeId;

    fn value_shape() -> SlotValueShape;
}
```

`ToLpValue` and `FromLpValue` stay as lower-level conversion traits for now.

### `#[derive(SlotValue)]`

Add a derive macro in `lpc-slot-macros`.

Default behavior:

- derive `SHAPE_ID` from `stringify!(TypeName)`
- infer `LpType` from the wrapped primitive or named public struct fields
- generate `ToLpValue`
- generate `FromLpValue`
- generate `SlotValue::value_shape`
- use `ValueEditorHint::Plain` unless an editor attribute is provided

Example:

```rust
#[derive(Clone, Copy, Debug, PartialEq, SlotValue)]
#[slot_value(editor = slider(min = 0.0, max = 1.0, step = 0.01))]
pub struct Ratio(pub f32);

pub type RatioSlot = ValueSlot<Ratio>;
```

Generated shape id:

```rust
SlotShapeId::from_static_name("Ratio")
```

### Auto Ids And Conflicts

The default slot value id is the Rust type name only.

That means this is an error:

```rust
mod a {
    #[derive(SlotValue)]
    pub struct Config(pub f32);
}

mod b {
    #[derive(SlotValue)]
    pub struct Config(pub u32);
}
```

Conflict detection belongs in `lpc-slot-codegen` discovery and registry insertion. The derive macro should not try to solve global discovery.

### Semantic Aliases

Most semantic slot files become small:

```rust
#[derive(Clone, Copy, Debug, PartialEq, SlotValue)]
#[slot_value(editor = slider(min = 0.0, max = 1.0, step = 0.01))]
pub struct Ratio(pub f32);

pub type RatioSlot = ValueSlot<Ratio>;
```

Some leaves may remain direct aliases when there is no semantic distinction:

```rust
pub type StringSlot = ValueSlot<String>;
```

But if editor metadata differs, use a semantic value:

```rust
#[derive(Clone, Debug, PartialEq, Eq, SlotValue)]
#[slot_value(editor = path)]
pub struct SourcePath(pub String);

pub type SourcePathSlot = ValueSlot<SourcePath>;
```

### Escape Hatches

Simple values use derive. Complex values can manually implement:

- `ToLpValue`
- `FromLpValue`
- `SlotValue`

Complex objects that are not leaf values should use `SlotRecord` or a public slot-data field and delegate to it.

Fully custom objects may manually provide:

- shape
- access
- get/set
- serialization/deserialization

That escape hatch should be explicit and rare.

## Behavioral Decisions

- No compatibility layer for old `FooSlot` internals.
- No manual ids unless implementation pressure forces it.
- No private fields in generated slot data.
- No `#[slot(skip)]` as a normal modeling tool.
- It is acceptable for this pass to break downstream code and fix it directly.
