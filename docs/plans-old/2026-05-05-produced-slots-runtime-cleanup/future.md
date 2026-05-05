## Structured Slot Values And Paths

- **Idea:** Introduce first-class structured slot values and slot-local value
  paths so a slot can expose structured data without making every nested field a
  separately versioned slot.
- **Why not now:** The current resolver still mostly resolves scalar/path-shaped
  values. Modeling structured slots well needs a clear `SlotValue` /
  `SlotPath` story, shape/type validation, and client diff rules.
- **Useful context:** Bindings should happen at the slot level, not within a
  value. `state.touches` has a version; `state.touches[3].id` does not have its
  own version. Nested paths are useful for reading, projection, and diffs, but
  the slot is the right unit for binding and version tracking.

## Slot-Level Binding Semantics

- **Idea:** Treat `SlotRef` as the bindable endpoint and version boundary, while
  `ValuePath` names nested data inside the current slot value.
- **Why not now:** This plan is already focused on runtime cleanup and may need
  transitional path-shaped resolver keys to keep behavior moving. The important
  constraint is to avoid documenting sub-value binding as the intended model.
- **Useful context:** A flat slot namespace means the slot names are flat; it
  does not mean all bindable or syncable data must be flattened into separate
  slots like `config_width` and `config_height`. During this plan a slot name
  may be an opaque string such as `config.width`; after this plan, revisit
  whether that should become structured `SlotPath`/`SlotValue` data.
