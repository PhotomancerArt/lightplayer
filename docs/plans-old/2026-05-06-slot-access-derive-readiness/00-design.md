# Slot Access Derive Readiness Design

## Scope Of Work

This plan makes the slot access model ergonomic enough to convert real LightPlayer source and runtime structs in a following plan.

In scope:

- Promote the mockup shape builder helpers into `lpc-model`.
- Add a `SlotRecordShape`-style non-root shape trait for Rust-authored record structs.
- Add a proc-macro crate for deriving slot record shape/access implementations.
- Use the derive in `lpc-slot-mockup` to remove most manual record boilerplate.
- Keep manual enum implementations for now unless a small, obvious helper is needed.
- Validate `lpc-model`, `lpc-slot-mockup`, `lpc-view`, and `lpc-wire` still work with shape sync, dynamic shader params, and client mutation.

Out of scope:

- Converting real `lpc-source` / `lpc-engine` structs to slots.
- Building enum derive support.
- Replacing semantic slot aliases with newtypes.
- Building server-side artifact mutation for real projects.

## File Structure

```text
lp-core/
  lpc-model/
    Cargo.toml
    src/
      lib.rs
      slot/
        mod.rs
        slot_access.rs
        slot_leaf.rs
        slot_record_shape.rs
        slot_shape.rs
        slot_shape_builder.rs
        slot_value.rs

  lpc-slot-macros/
    Cargo.toml
    src/
      lib.rs
      attr.rs
      record.rs

  lpc-slot-mockup/
    Cargo.toml
    src/
      model/
        mod.rs
      source/
        fixture_def.rs
        output_def.rs
        project_def.rs
        shader_def.rs
        texture_def.rs
      engine/
        fixture_node.rs
        output_node.rs
```

## Architecture Summary

`lpc-model` remains the runtime/home crate for slot concepts. It provides:

- `SlotShape`, `SlotValueShape`, `SlotShapeId`, and registry types.
- `SlotAccess`, `StaticSlotAccess`, and data access traits.
- Typed slot containers such as `SlotValue<T>`, `SlotMap<K, V>`, and `SlotOption<T>`.
- A public shape builder vocabulary under `lpc_model::slot::shape`.
- A new non-root record shape trait for generated and hand-authored inline record shapes.

`lpc-slot-macros` is a compile-time-only proc-macro crate. It generates normal Rust impls against `::lpc_model::...` paths. Generated code must not require `std`.

`lpc-slot-mockup` becomes the proving ground. It should use the derive for source and runtime records wherever that is reasonable, while preserving the existing sync and mutation tests.

## Main Components

### Shape Builder API

Move the useful helper vocabulary from `lpc-slot-mockup/src/model/shape_builder.rs` into `lpc-model`.

Target public path:

```rust
use lpc_model::slot::shape;

shape::id("source.shader");
shape::record(vec![...]);
shape::field("glsl_path", shape::leaf(source_path_shape()));
shape::map(SlotMapKeyShape::String, shape::reference(...));
shape::option(...);
shape::variant(...);
shape::value(ModelType::String);
shape::unit();
```

Keep these helpers in `lpc_model::slot::shape` rather than exporting all helper names at the crate root.

### Non-Root Record Shape Trait

Add a trait in `lpc-model`, likely:

```rust
pub trait SlotRecordShape {
    fn slot_record_shape() -> SlotShape;
}
```

This separates inline record shape generation from root registration:

- Inline records need `SlotRecordShape` and `SlotRecordAccess`.
- Root records additionally need `SlotAccess` and `StaticSlotAccess`.

### Record Derive

Add a new derive macro, likely re-exported as:

```rust
#[derive(lpc_model::SlotRecord)]
```

or, if root re-export naming becomes noisy:

```rust
#[derive(lpc_model::slot::SlotRecord)]
```

The derive generates:

- `impl SlotRecordShape for Type`
- `impl SlotRecordAccess for Type`
- if `#[slot(shape_id = "...")]` is present:
  - `impl SlotAccess for Type`
  - `impl StaticSlotAccess for Type`

Generated field access must use the same field order as generated shape construction.

### Attribute Shape

Start explicit. Do not rely on type alias inference.

Expected field annotations:

```rust
#[slot(value = ModelType::String)]
label: SlotValue<String>,

#[slot(leaf = source_path_shape())]
glsl_path: SourcePathSlot,

#[slot(record)]
compiler_options: CompilerOptions,

#[slot(map(key = "string", value_ref = "source.shader_param_def"))]
param_defs: SlotMap<String, ShaderParamDef>,

#[slot(option_ref = "source.scalar_hint")]
min: SlotOption<ScalarHint>,
```

Root annotation:

```rust
#[slot(shape_id = "source.shader")]
```

The `shape_id` string becomes:

```rust
const SHAPE_ID: SlotShapeId = SlotShapeId::from_static_name("source.shader");
```

### Mockup Conversion

Convert the mockup records first:

- Small roots:
  - `OutputDef`
  - `TextureDef`
  - `OutputNode`
- Inline records:
  - `CompilerOptions`
  - `ShaderParamDef`
  - `ScalarHint`
  - `TouchState`
  - `NodeInvocationDef`
- Larger roots:
  - `ProjectDef`
  - `ShaderDef`
  - `FixtureDef`
  - `FixtureNode`

Keep dynamic records manual:

- `ShaderNode` dynamic params shape/data should remain hand-authored for now because its field set is artifact-defined at runtime.

Keep enum data manual:

- `FixtureMapping` remains a manual `SlotEnumAccess` / `SlotRecordAccess` implementation until a later enum derive plan.

## Interactions

Shape registration should continue to happen through the existing registry:

```rust
Type::register_shape(&mut registry)?;
```

Generated root registration should call:

```rust
registry.register_tree(Self::SHAPE_ID, <Self as SlotRecordShape>::slot_record_shape())
```

Generated inline record fields should embed:

```rust
<CompilerOptions as SlotRecordShape>::slot_record_shape()
```

Generated root refs should embed:

```rust
shape::reference(shape::id("source.shader_param_def"))
```

Generated field access should return:

```rust
Some(SlotDataAccess::Value(&self.field))
Some(SlotDataAccess::Record(&self.field))
Some(SlotDataAccess::Map(&self.field))
Some(SlotDataAccess::Option(&self.field))
```

## Validation

Plan-level validation:

```bash
cargo test -p lpc-model
cargo check -p lpc-model --no-default-features
cargo check -p lpc-model --features schema-gen
cargo test -p lpc-slot-mockup
cargo test -p lpc-slot-mockup -- --nocapture --test-threads=1
cargo check -p lpc-view
cargo check -p lpc-wire --features schema-gen
git diff --check
```
