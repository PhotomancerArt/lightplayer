# Slots

Slots are the named data surface of the runtime.

A slot is the level where LightPlayer tracks:

- whether data is consumed or produced;
- binding targets and sources;
- revision/change information;
- generic sync and UI visibility.

Slots are not just plain fields. They are the boundary where authored data,
runtime state, binding resolution, and sync meet.

## Slot Roots

A slot root is a structured tree of slots. Examples:

- a node definition such as `ShaderDef`;
- a runtime state struct such as `ShaderState`;
- dynamic shader params materialized from authored param definitions.

Slot roots have shapes in the `SlotShapeRegistry`, so the engine and client can
walk them generically.

## Slot Leaves

A slot leaf contains a versioned value. The value may itself be structured, but
it changes as one logical unit from the slot system's point of view.

Example: `state.output` can be a slot leaf containing a `VisualProduct`.

## Slot Paths

Slot paths identify slots inside a slot root. They are separate from node-tree
paths and separate from value-internal paths.

This separation matters because a slot is the versioning and binding unit, while
a value may have its own internal structure.
