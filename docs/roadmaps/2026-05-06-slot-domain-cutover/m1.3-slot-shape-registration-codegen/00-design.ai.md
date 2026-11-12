# M1.3 Slot Shape Registration Codegen Design

## Scope Of Work

Build the static shape registration foundation needed before source defs become
real production slot roots.

M1.3 adds:

- idempotent static shape registration APIs,
- a `StaticSlotShape` trait,
- derive support for root static shapes,
- `OUT_DIR` build codegen for crate-level static shape bootstrap,
- mockup adoption and validation.

M1.3 does not convert `lpc-source`.

## File Structure

```text
lp-core/
  lpc-model/
    src/slot/
      slot_access.rs
      slot_shape.rs
      slot_shape_registry.rs
  lpc-slot-macros/
    src/
      attr.rs
      record.rs
  lpc-slot-codegen/                  # new build-time helper crate
    Cargo.toml
    src/lib.rs
  lpc-slot-mockup/
    build.rs
    src/lib.rs
    src/source/mod.rs
    src/engine/mod.rs
    src/model/mod.rs
    src/tests/
```

## Architecture Summary

Static slot roots are Rust-authored types. The type owns its static shape id and
can construct its root shape. A crate-level build script discovers those roots
and emits a bootstrap module into `OUT_DIR`.

```text
#[derive(SlotRecord)]
#[slot(root)]
pub struct ShaderDef { ... }
        |
        v
derive emits StaticSlotShape + StaticSlotAccess
        |
        v
build.rs scans src/**/*.rs for root slot records
        |
        v
OUT_DIR/slot_shapes.rs
        |
        v
register_all_static_slot_shapes(...)
ensure_static_slot_shape(...)
```

Dynamic shape roots stay separate:

```text
loaded shader artifact / node instance
        |
        v
runtime builds dynamic params shape
        |
        v
registry.register_tree(dynamic_shape_id, shape)
registry.replace_tree(dynamic_shape_id, shape)
```

## Main Components

### `StaticSlotShape`

`lpc-model` adds a trait for shape roots independent of data access:

```rust
pub trait StaticSlotShape {
    const SHAPE_ID: SlotShapeId;

    fn slot_shape() -> SlotShape;

    fn ensure_registered(
        registry: &mut SlotShapeRegistry,
    ) -> Result<bool, SlotShapeRegistryError>;
}
```

`ensure_registered` returns whether this call inserted/replaced nothing new:

- `Ok(true)` when a shape was newly inserted,
- `Ok(false)` when the same shape was already present,
- `Err(...)` when the id is registered with a different shape.

Exact naming can be adjusted during implementation, but the semantic boundary
should remain: idempotent static ensure, not dynamic replace.

### Registry Ensure

`SlotShapeRegistry` gets:

- `contains(id)`,
- `ensure_tree(id, shape)`,
- reference collection helpers or shape traversal helpers.

`ensure_tree` should be idempotent for identical shape trees and error for shape
id conflicts.

### Derive Output

For root records, `#[derive(SlotRecord)]` emits:

- `SlotRecordShape`,
- `SlotRecordAccess`,
- `FieldSlot`,
- `SlotAccess`,
- `StaticSlotShape`,
- `StaticSlotAccess` compatibility impl.

`StaticSlotAccess::register_shape` can stay as a compatibility shim delegating
to `StaticSlotShape::ensure_registered`.

### Build Codegen

Add a small std-only helper crate `lpc-slot-codegen`.

The helper:

- scans `src/**/*.rs`,
- parses files with `syn`,
- finds named structs with both `#[derive(SlotRecord)]` and `#[slot(root)]`,
- infers a public type path from the file path and type name,
- writes `slot_shapes.rs` to the caller-provided output path.

Path inference convention:

- `src/source/project_def.rs` type `ProjectDef` -> `crate::source::ProjectDef`
  by assuming concept files re-export their headline type from the parent module.
- `src/node/project/mod.rs` type `ProjectDef` ->
  `crate::node::project::ProjectDef`.
- `src/lib.rs` root type -> `crate::TypeName`.

Generated API:

```rust
pub fn register_all_static_slot_shapes(
    registry: &mut ::lpc_model::SlotShapeRegistry,
) -> Result<(), ::lpc_model::SlotShapeRegistryError>;

pub fn ensure_static_slot_shape(
    registry: &mut ::lpc_model::SlotShapeRegistry,
    id: ::lpc_model::SlotShapeId,
) -> Result<bool, ::lpc_model::SlotShapeRegistryError>;
```

`ensure_static_slot_shape` should:

1. match the id against discovered static roots,
2. call `<Type as StaticSlotShape>::ensure_registered(registry)`,
3. collect references from the registered shape,
4. recursively ensure referenced static shapes when the generated module knows
   them,
5. return `Ok(false)` only for top-level unknown ids.

If a known static shape references an unknown missing id, return a registry
error rather than silently producing an incomplete registry.

### Mockup Adoption

`lpc-slot-mockup` gets a `build.rs` and includes generated slot shape code.

Manual lists in `source::register_shapes`, `engine::register_shapes`, and
`model::register_shapes` should be removed or reduced to wrappers around the
generated functions.

`ShaderNode` dynamic shape registration remains manual and should be documented
as dynamic-instance-only.

## Important Constraints

- Keep `lpc-model` `no_std + alloc`.
- Keep `lpc-slot-codegen` std-only and build-time only.
- Do not use `inventory`, linker sections, runtime reflection, or global
  constructors.
- Do not introduce source def conversion in M1.3.
- Do not make dynamic shape ids static type ids when shape can vary per loaded
  artifact or node instance.
