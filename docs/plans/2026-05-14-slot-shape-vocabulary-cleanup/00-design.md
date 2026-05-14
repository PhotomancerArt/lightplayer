# Design: Slot Shape Vocabulary Cleanup

## Scope Of Work

Clarify the slot system vocabulary and migrate code toward shape/path-root naming:

- Registry APIs should talk about shapes, not roots.
- Path compilation may keep `root` where it means root of the path traversal.
- `SlotAccess` should be documented as the runtime slot object/data-access
  trait.
- Design docs should stop saying persisted objects must be slot roots.
- Private codegen metadata should stop using root where it means codec target
  or shape target.
- Type-level `#[slot]` should become the preferred spelling for slot-annotated
  Rust types.
- Slot-annotated records should get `StaticSlotShape` by default.

Out of scope:

- Redesigning mutation/sync object naming.
- Changing runtime behavior or wire formats.
- Renaming `SlotShape` to `SlotSchema`.

## File Structure

```text
docs/design/slots/
  overview.md
  serialization.md
  values.md

lp-core/lpc-model/src/slot/
  slot_access.rs
  slot_accessor.rs
  slot_lookup.rs
  slot_record_shape.rs
  slot_shape.rs
  slot_shape_builder.rs
  slot_shape_registry.rs

lp-core/lpc-slot-macros/src/
  attr.rs
  lib.rs
  record.rs

lp-core/lpc-slot-codegen/src/
  lib.rs
```

## Architecture Summary

The cleaned-up vocabulary separates definition, registration, and usage:

```text
Rust slot type
        |
        v
SlotShape / SlotRecordShape / SlotEnumShape
        |
        v
registered shape in SlotShapeRegistry
        |
        v
path root for SlotAccessor / SlotPath resolution
        |
        v
SlotAccess runtime object supplied by engine/wire/storage use site
```

`SlotShapeRegistry` is a schema catalog. It does not own runtime objects and
does not decide what is top-level in the app.

`SlotAccessor` compiles a `SlotPath` against a registered path-root shape. When
used, it receives a `SlotAccess` value and verifies that the runtime object's
shape id matches the compiled path root.

Engine, storage, and wire layers can still have runtime roots or object roots,
but those names belong to those layers.

## Main Components

### SlotShapeRegistry API

Add shape-named APIs and update internal docs:

- `register_shape`
- `register_shape_named`
- `register_shape_with_version`
- `register_shape_named_with_version`
- `ensure_shape`
- `ensure_shape_named`
- `ensure_shape_with_version`
- `ensure_shape_named_with_version`
- `replace_shape`
- `replace_shape_named`
- `replace_shape_with_version`
- `replace_shape_named_with_version`
- `unregister_shape`
- `unregister_shape_with_version`

Prefer migrating call sites and removing old root-named registry methods. Keep
compatibility wrappers only if removing them creates noisy scope expansion.

### StaticSlotShape And SlotAccess Docs

Rewrite docs so:

- `StaticSlotShape` means a Rust type owns a stable registered shape.
- `SlotAccess` means a runtime slot object exposes data for a shape id.
- `StaticSlotAccess` is just the ergonomic combination of both.
- Type-level `#[slot]` is the preferred way to opt a Rust type into static slot
  shape generation.
- `#[slot(root)]` is compatibility wording during migration, not the conceptual
  center.

Avoid claiming these are necessarily top-level persisted/synchronized objects.

### SlotAccessor Path-Root Naming

Keep `root` fields/parameters in `SlotAccessor` if the docs and diagnostics make
clear it means the root shape of the path traversal.

Preferred diagnostics:

- "missing slot path root shape ..."
- "slot accessor path root ... does not match data shape ..."

`SlotPath::root()` stays as-is. There "root path" means "empty path at the
start of the current tree."

### Macro Behavior

Update `SlotRecord` derive so all slot-annotated records get static shape
support by default.

Preferred direction:

- Type-level `#[slot]` means the type participates in the slot system and gets
  static shape support.
- `#[slot(root)]` remains only as a compatibility alias if needed.
- All `SlotRecord` types should continue to implement `SlotRecordShape`,
  `SlotRecordAccess`, `FieldSlot`, and `SlotMapValueAccess`.
- Static shape ids should be stable and predictable. Use the existing explicit
  `shape_id` override where present; otherwise continue using module path plus
  type name unless the implementation needs a stronger policy.

### Codegen Naming

Rename private codegen structs/functions where root means generated target:

- `StaticSlotRoot` -> `StaticRegisteredShape`
- `discover_static_slot_roots` -> `discover_static_registered_shapes`
- `SlotCodecRoot` -> `SlotCodecType`
- `render_slot_codec_root*` -> `render_slot_codec_type*`
- `mockup_source_codec_module().roots` -> `.types`

Generated public function names do not need to change.

### Design Docs

Update `docs/design/slots`:

- Replace `Slot Root` as a main shape concept with `Registered Shapes`, `Path
  Roots`, and `Runtime Slot Objects`.
- Explain that `SlotShape` is the slot schema node.
- Explain that runtime/storage/wire layers may choose object roots.
- Update serialization docs to say generated adapters target slot-modeled
  types, not necessarily roots.
- Keep note that independent persisted objects should be modeled with slots,
  but do not imply they must be registry roots by definition.

## Compatibility Strategy

This is mainline development, so prefer clean vocabulary over long-lived aliases.

Recommended order:

1. Add new shape API names.
2. Migrate internal call sites.
3. Migrate type annotations toward type-level `#[slot]` where practical.
4. Keep old root methods/attributes as deprecated wrappers only where they
   prevent needless breakage.
5. Update docs and tests to prefer new names.
