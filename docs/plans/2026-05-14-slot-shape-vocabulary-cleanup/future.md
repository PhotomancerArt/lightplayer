# Future Work

## Consider Schema Naming

- **Idea:** Rename shape terminology to schema terminology, for example
  `SlotSchema`, `SlotSchemaId`, and `SlotSchemaRegistry`.
- **Why not now:** `shape` is deeply embedded in current code and still
  communicates the concept well. The user does not strongly prefer schema.
- **Useful context:** `SlotShape`, `SlotShapeId`, `SlotShapeRegistry`, and
  design docs can describe a shape as the slot schema node without renaming
  every type.

## Runtime Object Directory

- **Idea:** Introduce a use-site owned directory for runtime slot objects, with
  names like `source.shader` mapping to `&dyn SlotAccess`.
- **Why not now:** The mock runtime has an ad hoc `roots()` method, and the real
  runtime/storage/wire design needs its own pass.
- **Useful context:** `lp-core/lpc-slot-mockup/src/engine/runtime.rs`.
