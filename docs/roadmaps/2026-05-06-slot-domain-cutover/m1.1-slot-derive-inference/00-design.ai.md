# M1.1 Slot Derive Inference Design

## Scope Of Work

Make slot record derives clean enough for real source-domain structs.

M1.1 changes the derive from "every included field must have a shape attribute" to "every named field is a slot unless skipped, and its type must prove how it maps to slots."

## File Structure

```text
lp-core/lpc-model/src/slot/
  slot_access.rs        # FieldSlot trait
  value_slot.rs         # generic ValueSlot/MapSlot/OptionSlot
  slots/                # semantic field slot newtypes
  mod.rs                # exports

lp-core/lpc-slot-macros/src/
  attr.rs               # parse root/name/skip/infer attrs
  record.rs             # infer field shape/access and root ids

lp-core/lpc-model/tests/
  slot_record_derive.rs # inferred-field derive tests

lp-core/lpc-slot-mockup/src/
  source/*.rs
  engine/*.rs           # remove redundant derive attrs where inference is exact
```

## Architecture Summary

`SlotRecord` derives use a new model trait:

```rust
pub trait FieldSlot {
    fn slot_field_shape() -> SlotShape;
    fn slot_field_data(&self) -> SlotDataAccess<'_>;
}
```

The derive includes named fields by default:

```rust
#[derive(lpc_model::SlotRecord)]
#[slot(root)]
struct ShaderDef {
    glsl_path: SourcePathSlot,

    #[slot(skip)]
    cache: CachedThing,
}
```

Generated code calls:

```rust
<#field_ty as ::lpc_model::FieldSlot>::slot_field_shape()
<#field_ty as ::lpc_model::FieldSlot>::slot_field_data(&self.field)
```

If a field type is not slot-mappable, the missing `FieldSlot` impl is the validation error. Authors opt out with `#[slot(skip)]`.

Root records use:

```rust
#[slot(root)]
```

and infer:

```rust
SlotShapeId::from_static_name(concat!(module_path!(), "::", stringify!(TypeName)))
```

`#[slot(shape_id = "...")]` remains available and implies root behavior for compatibility.

Explicit field shape attrs remain as compatibility overrides, but semantic slot aliases have been promoted to real newtypes so common domain fields infer their own metadata.

## Main Components

- `FieldSlot` is the field-level inference trait.
- `ValueSlot<T>` implements `FieldSlot` when `T: SlotLeaf`.
- `MapSlot<K,V>` implements `FieldSlot` when `K: MapSlotKeyLike` and `V` exposes slot value access.
- `OptionSlot<T>` implements `FieldSlot` for optional slot values.
- Semantic field slots such as `RatioSlot`, `Dim2uSlot`, and `RelativeNodeRefSlot` implement `FieldSlot` directly.
- Every derived record implements `FieldSlot`, so nested record fields infer naturally.
- Existing explicit derive attrs continue to work:
  - `#[slot(skip)]`
  - `#[slot(name = "...")]`
  - `#[slot(value = ...)]`
  - `#[slot(leaf = ...)]`
  - `#[slot(record)]`
  - `#[slot(enum)]`
  - `#[slot(map(...))]`
  - `#[slot(option_ref = ...)]`

## Constraints

- Preserve `no_std + alloc`.
- Do not convert real `lpc-source` defs in M1.1.
- Do not introduce broad real-domain conversion in M1.1.
- Do not weaken tests or suppress warnings.
