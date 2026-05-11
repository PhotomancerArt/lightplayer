# M1 Design: Shared Slot Model Foundation

## Scope Of Work

Build the `lpc-model` foundation for slot-shaped, versioned structured data.

In scope:

- Add rootable slot paths and update slot references.
- Add slot data and slot tree instance types.
- Add slot shape and registry types.
- Add `ResourceRef` to `ModelValue` / `ModelType`.
- Add basic recursive shape/data validation.
- Add tests and rustdocs documenting the semantics.

Out of scope:

- Source node definition migration.
- Runtime node exposure.
- Resolver/binding changes.
- Wire/view sync changes.
- Static/dynamic authoring helpers beyond minimal constructors.
- Derive macros.
- Artifact mutation APIs.

## File Structure

```text
lp-core/lpc-model/src/
  lib.rs
  prop/
    model_type.rs
    model_value.rs
    mod.rs
  resource.rs
  slot/
    mod.rs
    slot_name.rs
    slot_owner.rs
    slot_path.rs
    slot_ref.rs
    value_ref.rs
    slot_meta.rs
    slot_shape.rs
    slot_registry.rs
    slot_data.rs
    slot_tree.rs
```

Existing `slot_name.rs`, `slot_owner.rs`, `slot_ref.rs`, and `value_ref.rs`
should stay, but `slot_ref.rs` and `value_ref.rs` need to move from flat
`SlotName` identity to `SlotPath`.

## Architecture Summary

The model has four layers:

```text
SlotOwner
  who owns the slot namespace

SlotTree
  rooted instance data for one owner

SlotData
  one node inside the tree, shaped and optionally versioned

SlotShape
  schema and UI metadata for SlotData
```

References use:

```text
SlotRef = SlotOwner + SlotPath
ValueRef = SlotRef + ValuePath
```

`SlotPath` addresses nodes in a slot tree. `ValuePath` is only for projection
inside a leaf `ModelValue`.

`SlotRegistry` owns shapes:

```text
SlotShapeId -> complete SlotShape tree
```

A registered shape is complete. M1 must not introduce internal shape id
references inside registered shapes.

## Main Components

### SlotPath

`SlotPath` is a sequence of `SlotName` segments. It should support:

- `SlotPath::root()`
- `SlotPath::parse("config.mapping")`
- `segments()`
- `is_root()`
- `join(...)` or `child(...)` helper if useful
- display / serde as a string

Parsing should reject empty text, while `SlotPath::root()` represents the root
path explicitly.

### SlotRef And ValueRef

`SlotRef` becomes:

```rust
pub struct SlotRef {
    pub owner: SlotOwner,
    pub path: SlotPath,
}
```

`ValueRef` continues to combine a `SlotRef` with a `ValuePath`. It should be
documented as projection/inspection, not a binding endpoint or version
boundary.

### ModelValue Resource

Add:

```rust
ModelValue::Resource(ResourceRef)
ModelType::Resource
```

Resource payload bytes remain outside `ModelValue`. `ResourceRef` is ordinary
portable model data.

### SlotShape

M1 defines the initial vocabulary:

```rust
pub enum SlotShape {
    Value { meta: SlotMeta, ty: ModelType },
    Record { meta: SlotMeta, fields: Vec<SlotFieldShape> },
    Map { meta: SlotMeta, key: SlotMapKeyShape, value: Box<SlotShape> },
    Enum { meta: SlotMeta, variants: Vec<SlotVariantShape> },
    Option { meta: SlotMeta, some: Box<SlotShape> },
}
```

The exact names may be adjusted, but the semantics should hold.

No array or tuple variants in M1.

### SlotData

M1 defines matching instance data:

```rust
pub enum SlotData {
    Value(Versioned<ModelValue>),
    Record(SlotRecord),
    Map(SlotMap),
    Enum(SlotEnum),
    Option(SlotOption),
}
```

Suggested supporting shapes:

```rust
pub struct SlotRecord {
    pub fields: Vec<SlotData>,
}

pub struct SlotMap {
    pub entries: BTreeMap<SlotMapKey, SlotData>,
}

pub struct SlotEnum {
    pub variant: SlotName,
    pub data: Box<SlotData>,
}

pub enum SlotOption {
    None,
    Some(Box<SlotData>),
}
```

Use a constrained `SlotMapKey` if `ModelValue` is too broad to serve as an
ordered key. Map keys are stable identities for dynamic collections, not
arbitrary payload values.

Records are indexed against their shape. Field `i` in `SlotRecord.fields`
corresponds to field `i` in `SlotShape::Record.fields`. This avoids duplicating
field names in every data instance and keeps the shape as the structure owner.

Map keys are constrained stable identities:

```rust
pub enum SlotMapKey {
    String(String),
    I32(i32),
    U32(u32),
}

pub enum SlotMapKeyShape {
    String,
    I32,
    U32,
}
```

### SlotTree

`SlotTree` is the rooted data object:

```rust
pub struct SlotTree {
    pub shape: SlotShapeId,
    pub root: SlotData,
}
```

It should provide basic traversal:

- `get(&SlotRegistry, &SlotPath) -> Option<&SlotData>`
- possibly `get_mut(&SlotPath) -> Option<&mut SlotData>`

Because record data stores fields by index, traversal needs access to the
registered shape tree to map path segments back to record field positions.

Traversal through `Enum` and `Option` should be conservative in M1. It is fine
to support record/map traversal first and keep enum/option traversal helper
semantics minimal if validation still covers them.

### SlotRegistry

`SlotRegistry` stores registered shape trees:

```rust
pub struct SlotShapeId(String);
pub struct SlotRegistry { ... }
```

It should support:

- registering a shape by id,
- replacing or rejecting duplicate ids by a clear policy,
- lookup by id,
- tests that a registered id owns a complete shape tree.

Suggested initial policy: duplicate registration returns the old shape or errors
explicitly; choose whichever matches existing crate style. Do not silently
ignore duplicate ids.

### Validation

Add basic recursive validation:

```rust
registry.validate_tree(&SlotTree) -> Result<(), SlotError>
```

or an equivalent API. M1 validation should check:

- tree shape id exists,
- `SlotData` variant matches `SlotShape` variant,
- `ModelValue` matches `ModelType`,
- record field count matches shape fields and indexed fields match the corresponding shape field,
- map keys match SlotMapKeyShape and values match map value shape,
- enum variant exists and variant data matches,
- option `Some` matches the some shape and `None` is valid.

Validation should produce useful errors but does not need a perfect final error
taxonomy.

## Validation Approach

Primary validation:

```bash
cargo test -p lpc-model
```

Compile fallout check because `SlotRef` and `ModelValue` are shared:

```bash
cargo test -p lpc-source
cargo test -p lpc-wire
cargo test -p lpc-view
```

Do not run `cargo test --workspace`.
