# Rename Static Registration API

## Smell

`StaticSlotShape::ensure_registered` still exists and has a broad name. After
the generated static catalog migration, most authored static shapes should be
looked up through the catalog, not inserted into `SlotShapeRegistry`.

The method is still useful for runtime-state shapes, test-only shapes, and
crates that do not have their own catalog wired into `SlotShapeLookup`.

## Better Shape

Rename or split the API so call sites make the intent explicit.

Possible directions:

- `ensure_runtime_registry_shape`
- `ensure_dynamic_registry_shape`
- a separate trait for runtime-state static shapes

Avoid names that make bulk registration of authored static catalog shapes feel
normal again.

## Useful Context

- `lp-core/lpc-model/src/slot/slot_access.rs`
- engine `register_runtime_state_shapes` hooks
- `lpc-slot-mockup` explicit test registry setup
