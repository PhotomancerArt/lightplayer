# M1.1 Slot Derive Inference Notes

## Scope Of Work

M1.1 prepares the slot derive for real source-domain structs before M2 converts `lpc-source`.

In scope:

- Make `#[derive(SlotRecord)]` infer slot field shape/access from field types.
- Treat named fields as slot fields by default.
- Add `#[slot(skip)]` as the opt-out for helper/cache/non-domain fields.
- Validate fields by generated trait bounds: if a field is not slot-mappable and is not skipped, compilation should fail.
- Add `#[slot(root)]` to generate `SlotAccess + StaticSlotAccess`.
- Infer root `SlotShapeId` from the Rust type identity by default.
- Preserve an explicit `#[slot(shape_id = "...")]` escape hatch.
- Convert the mockup/test derives to the new syntax enough to prove the ergonomics before M2.

Out of scope:

- Converting real `lpc-source` defs.
- Authored serde for `ValueSlot`, `MapSlot`, and `OptionSlot` unless a small piece is required for inference tests.
- Changing slot wire format.
- Changing runtime node behavior.
- Designing full metadata attributes beyond allowing a future extension point.

## User Notes And Decisions

- User is skeptical of requiring `#[slot]` on every field when the field type already says it is a slot.
- Decision: `SlotRecord` derive should be opt-out. All named fields are slots unless marked `#[slot(skip)]`.
- Decision: the macro should validate fields. If a field has no auto-mappable slot type, it should be a compile error unless explicitly skipped.
- Decision: shape ids should not require strings like `source.shader`. Prefer type-derived ids; explicit ids remain available.
- Static shape id stability across builds is not critical because the server owns the registry and sends shape ids to clients.
- The main reason for this M1.1 is to make M2 source defs clean, domain-native, and not buried under redundant macro annotations.

## Starting Codebase State

### Macro

- `lp-core/lpc-slot-macros/src/record.rs` started with:
  - parses `#[slot(shape_id = "...")]` as the only root mechanism,
  - requires each included field to have a `#[slot(...)]` attribute,
  - uses the field attribute to generate both shape and access,
  - supports `value`, `leaf`, `record`, `enum`, `map`, `option_ref`, and `skip`.
- `lp-core/lpc-slot-macros/src/attr.rs` returned an error when a field had no `#[slot(...)]`.
- Root shape ids were string literals such as `"source.shader"`.

### Model Traits

- `lpc-model/src/slot/slot_access.rs` defines:
  - `SlotAccess`
  - `StaticSlotAccess`
  - `SlotDataAccess`
  - `SlotValueAccess`
  - `SlotRecordAccess`
  - `SlotMapAccess`
  - `SlotEnumAccess`
  - `SlotOptionAccess`
- `lpc-model/src/slot/slot_record_shape.rs` defines `SlotRecordShape`.
- `lpc-model/src/slot/slot_enum_shape.rs` defines `SlotEnumShape`.
- `lpc-model/src/slot/value_slot.rs` defines:
  - `ValueSlot<T>`
  - `MapSlot<K,V>`
  - `OptionSlot<T>`
  - `MapSlotKeyLike`
  - `SlotMapValueAccess`
- `lpc-model/src/slot/slot_leaf.rs` defines:
  - `SlotLeaf`
  - `SlotValueShape`
- `lpc-model/src/slot/slots/` now holds semantic leaf newtypes and shape helpers.

### Tests And Mockup Usage

- `lpc-model/tests/slot_record_derive.rs` used the old verbose syntax.
- `lpc-slot-mockup` used the old verbose syntax widely.
- The mockup is a good proving ground for derive inference because it already has:
  - nested records,
  - roots,
  - maps,
  - options,
  - enum fields with hand-authored enum access.

## Proposed Trait Shape

Add one or two traits in `lpc-model`:

```rust
pub trait FieldSlot {
    fn slot_field_shape() -> SlotShape;
    fn slot_field_data(&self) -> SlotDataAccess<'_>;
}
```

Implement for:

- `ValueSlot<T>` where `T: SlotLeaf`
- `MapSlot<K,V>` where `K: MapSlotKeyLike` and `V` can expose a map value shape/access
- `OptionSlot<T>` where `T` can expose an option value shape/access
- record structs where `T: SlotRecordShape + SlotRecordAccess`
- semantic field slots such as `RatioSlot`, `Dim2uSlot`, `Affine2dSlot`, and `RelativeNodeRefSlot`.
- hand-authored enum structs can implement `FieldSlot` directly when they are used as fields.

The macro should generate code that calls:

```rust
<#field_ty as ::lpc_model::FieldSlot>::slot_field_shape()
<#field_ty as ::lpc_model::FieldSlot>::slot_field_data(&self.field)
```

If the type does not implement `FieldSlot`, compile fails naturally.

## Proposed Macro Syntax

Default inferred record:

```rust
#[derive(lpc_model::SlotRecord)]
struct NestedRecord {
    count: ValueSlot<u32>,
}
```

Root record with inferred id:

```rust
#[derive(lpc_model::SlotRecord)]
#[slot(root)]
struct ShaderDef {
    glsl_path: SourcePathSlot,
    texture_loc: RelativeNodeRefSlot,
}
```

Explicit id escape hatch:

```rust
#[derive(lpc_model::SlotRecord)]
#[slot(root, shape_id = "ShaderDef")]
struct ShaderDef {
    glsl_path: SourcePathSlot,
}
```

Skipped helper field:

```rust
#[derive(lpc_model::SlotRecord)]
struct WithCache {
    value: ValueSlot<u32>,
    #[slot(skip)]
    cache: CachedThing,
}
```

Possible field-name override:

```rust
#[slot(name = "glsl_opts")]
compiler_options: GlslOptsDef,
```

Suggested: support `name` in M1.1 if cheap, because it is useful for source-domain serde/slot naming alignment. Otherwise mark as future.

## Root Shape Id Inference

Generate `StaticSlotAccess::SHAPE_ID` from Rust type identity.

Suggested generated expression:

```rust
::lpc_model::SlotShapeId::from_static_name(concat!(module_path!(), "::", stringify!(TypeName)))
```

This is stable enough within one server build, collision-resistant enough for static startup registration, and avoids manually maintaining registry-style strings.

For tests that need stable expected ids, assert that `record.shape_id() == Type::SHAPE_ID` rather than expecting a numeric id.

## Open Questions

### Q1. Should `#[slot(root)]` be required for root impls, or should every derived record get `StaticSlotAccess`?

Context: nested records need `SlotRecordShape + SlotRecordAccess`; roots additionally need `SlotAccess + StaticSlotAccess`.

Suggested answer: require `#[slot(root)]`. It keeps root registration intentional while letting nested records remain lightweight.

### Q2. Should M1.1 support `#[slot(name = "...")]`?

Context: field names are inferred from Rust identifiers. Real source defs may want Rust name and slot name to diverge, though current likely examples can use matching names.

Suggested answer: include it if straightforward. It is low-risk and helps future source conversion.

### Q3. Should old explicit field forms remain?

Context: existing mockup uses explicit field forms. Removing all at once creates churn but may be good cleanup.

Suggested answer: keep explicit forms as overrides for now, but migrate mockup/test examples to inferred forms where possible. The derive should no longer require them.

### Q4. How should maps infer value shapes?

Context: current map attr requires `value_ref = "source.shader_param_def"` and builds a reference shape. Inference needs to derive a shape from `V`.

Suggested answer: add a value-shape trait for map/option children. For record values, use inline `SlotRecordShape::slot_record_shape()` by default unless the value type also has `StaticSlotAccess` and should be referenced. Defer reference-vs-inline tuning unless it blocks tests.

## Validation Commands

```bash
cargo fmt
cargo test -p lpc-model --lib --tests
cargo test -p lpc-slot-mockup --lib --tests
cargo check -p lpc-engine -p lpa-client -p lpa-server -p lp-cli
```

If macro changes require broader host checking:

```bash
cargo clippy -p lpc-model --all-targets -- -D warnings
cargo clippy -p lpc-slot-mockup --all-targets -- -D warnings
```
