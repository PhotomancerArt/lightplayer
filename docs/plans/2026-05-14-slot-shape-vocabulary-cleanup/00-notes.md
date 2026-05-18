# Notes: Slot Shape Vocabulary Cleanup

## Scope Of Work

Clean up the slot vocabulary so "root" no longer means "any registered shape."

The target model:

- Any slot-annotated Rust type should have a registered shape.
- `SlotAccess` is the generic runtime slot object/data-access concept.
- A registered shape id can be used as the root of a path: the starting schema
  for resolving a `SlotPath`.
- "Root" belongs to use sites that choose addressable runtime/storage/wire
  objects, or to path terminology where it literally means the root of the
  current path tree.

This plan should rename docs, APIs, generated code, and tests toward that model
without taking on broader serialization/codegen work.

## Current State

The code currently uses "root" in several different ways:

- `#[slot(root)]` causes `SlotRecord` derive to emit:
  - `SlotAccess`
  - `StaticSlotShape`
  - `StaticSlotAccess`
- `SlotShapeRegistry` stores id-addressed shapes but calls them roots:
  - `register_root`
  - `ensure_root`
  - `replace_root`
  - `unregister_root`
- `SlotAccessor::compile(root, path, registry)` uses `root` as the starting
  `SlotShapeId` for path compilation.
- `SlotShape::Ref` docs say it references a registered root shape.
- `lpc-slot-codegen` discovers only `#[slot(root)]` records for generated
  shape bootstrap and names the internal model `StaticSlotRoot`.
- Design docs describe slot roots as top-level persisted/synchronized domain
  objects, which is not what the shape code actually enforces.
- Mock runtime has a `roots()` method that returns named `&dyn SlotAccess`
  objects. That is a runtime/app convention, not a shape-system concept.

Important files:

- `lp-core/lpc-model/src/slot/slot_access.rs`
- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`
- `lp-core/lpc-model/src/slot/slot_accessor.rs`
- `lp-core/lpc-model/src/slot/slot_lookup.rs`
- `lp-core/lpc-model/src/slot/slot_shape.rs`
- `lp-core/lpc-model/src/slot/slot_shape_builder.rs`
- `lp-core/lpc-model/src/slot/slot_record_shape.rs`
- `lp-core/lpc-slot-macros/src/lib.rs`
- `lp-core/lpc-slot-macros/src/record.rs`
- `lp-core/lpc-slot-codegen/src/lib.rs`
- `docs/design/slots/overview.md`
- `docs/design/slots/serialization.md`
- `docs/design/slots/values.md`

## User Notes

- The current root concept feels fuzzy and hard to defend in review.
- Most listed "root" behaviors are usage-layer concerns, not shape-definition
  concerns.
- "Top-level sync object" is not currently a generic shape-system concept.
- The engine can choose a node/object and then do slot-related work on it.
- There is no master store of all runtime roots today.
- The original root idea may have meant "shape you drill down into," but that
  may not need a special root category.
- `SlotAccess` may already be the right generic term for a runtime slot object.
- Type-level `#[slot]` feels like the clean preferred spelling; `root` is too
  loaded.
- The instinct is to keep it simple: all slot-annotated Rust types should get
  static shapes.
- Registry APIs should definitely be shape based.
- `root` may be fine in `SlotAccessor` because there it can mean "root of the
  path" contextually.
- `SlotAccess` is acceptable. Alternatives like `Slotted` or `SlotObj` do not
  obviously feel more Rusty.
- We should not shy away from large renames during mainline development.
- `schema` is worth considering as an alternate term to `shape`, but `shape` is
  already deeply embedded in current code and user preference is not strong.

## Open Questions

### Q1. Should `#[slot(root)]` be renamed?

Answer: yes. Prefer type-level `#[slot]` as the clean spelling.

Context:

- The attribute is widely used and currently means "also emit `StaticSlotShape`
  and `SlotAccess`."
- Since all slot-annotated Rust types should get static shapes, a bare
  type-level `#[slot]` can mean "this type participates in the slot system."
- If a future use site needs object-root behavior, that should be modeled in
  the runtime/storage/wire layer, not as `#[slot(root)]`.
- Keep `#[slot(root)]` only as a temporary compatibility spelling if that makes
  migration easier.

### Q2. Should `StaticSlotShape` apply to all `SlotRecord` types?

Answer: yes, keep it simple.

Context:

- The user's desired direction is that any slot-annotated Rust type should be
  eligible for the registry.
- Today, inline records implement `SlotRecordShape` and `FieldSlot`, but not
  `StaticSlotShape` unless marked `#[slot(root)]` or given a `shape_id`.
- Making every record derive a stable shape id would affect ids, generated
  registration, and possibly binary/codegen surface.
- Despite that churn, the simpler model is better: all slot-annotated records
  should have stable static shapes and be eligible for registration.
- This plan should include that behavior unless implementation reveals a real
  blocker.

### Q3. What should replace "root" in `SlotShapeRegistry` APIs?

Answer: `shape`.

Context:

- The registry stores `SlotShapeId -> SlotShapeEntry`.
- The value being registered is a shape, not an object instance.
- Rename public methods toward:
  - `register_shape`
  - `register_shape_named`
  - `ensure_shape`
  - `ensure_shape_named`
  - `replace_shape`
  - `replace_shape_named`
  - `unregister_shape`
- Prefer removing old root-named methods once call sites are migrated, unless a
  compatibility shim prevents needless breakage during the transition.

### Q4. What should replace "root" inside `SlotAccessor`?

Answer: keep `root` where it means root of the path.

Context:

- `SlotAccessor::compile(root, path, registry)` does not need a runtime object.
  It needs the shape id to start path resolution.
- `SlotAccessor::access(root: &dyn SlotAccess, ...)` does use a runtime object,
  but only checks that its `shape_id()` matches the compiled path root.
- In this context, `root` can be reasonable because it means "root of this path
  traversal," not "top-level app object."
- Keep or clarify wording in diagnostics so it says "path root shape" when
  useful.

### Q5. Should `SlotAccess` be renamed?

Answer: no.

Context:

- `SlotAccess` is already a good generic runtime slot object trait:
  `shape_id()` plus `data()`.
- It does not encode ownership, persistence, sync, or top-levelness.
- Alternatives like `Slotted` or `SlotObj` do not feel clearly better.
- Updating docs around `SlotAccess` should be enough.

### Q6. Should `SlotCodecRoot` be renamed?

Answer: yes. Be bold with large renames while the project is in mainline
development.

Context:

- The current `SlotCodecRoot` is a generated adapter target for a top-level
  codec function.
- In the new vocabulary, a better name is likely `SlotCodecType` or
  `SlotCodecShape`.
- Since this type is private to `lpc-slot-codegen`, it can be renamed cheaply.

### Q7. Should the system use "schema" instead of "shape"?

Suggested answer: keep `shape` for now, optionally define it as the slot schema.

Context:

- "Schema" is a professional, broadly understood term.
- "Shape" is already pervasive in code: `SlotShape`, `SlotShapeId`,
  `SlotShapeRegistry`, `StaticSlotShape`.
- Renaming shape to schema would be a very large mechanical change, and the
  user does not strongly care.
- A good compromise is docs language like "SlotShape is the schema node."

## Suggested Direction

Use this vocabulary:

- **Slot shape:** schema node.
- **Registered shape:** schema stored in `SlotShapeRegistry` by `SlotShapeId`.
- **Path root:** registered shape id used as the starting point for `SlotPath`
  resolution.
- **SlotAccess:** runtime value/object that exposes data for a shape id.
- **Runtime root / object root:** use-site concept owned by engine/wire/storage,
  not by the shape registry.

The implementation should be bold about renames and only preserve compatibility
where it prevents needless breakage during a multi-phase migration.
