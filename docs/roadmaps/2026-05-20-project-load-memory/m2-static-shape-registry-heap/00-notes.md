# M2 Notes: Static Slot Shape Registry Heap

## Scope

Replace per-engine resident static slot shape registration with a generated
static catalog plus a runtime dynamic overlay. The implementation should pursue
the clean path rather than a quick owned-shape fallback: static authored shapes
should live as generated read-only descriptors, not as `SlotShape` values cloned
into every `SlotShapeRegistry`.

This milestone must preserve on-device project loading, TOML slot parsing,
runtime lookup/mutation, slot sync, and the on-device GLSL compiler path.

## User Direction

- Focus on the better path, not the quick path.
- Static slot shapes are already known at build/codegen time.
- The current codegen writes static shapes into an in-memory registry, which is
  duplicative on embedded.
- Wire compatibility can be relaxed if needed, but failure should be graceful
  when client/server static catalogs do not match.
- Runtime still needs to look up and interact with static and dynamic shapes.
- A generated match on shape id is acceptable and likely desirable.

## Current Code Shape

- `lp-core/lpc-model/build.rs` generates `OUT_DIR/slot_shapes.rs` and
  `OUT_DIR/slot_views.rs` through `lpc-slot-codegen`.
- `lp-core/lpc-slot-codegen/src/render/slot_shapes.rs` currently emits:
  - `register_all_static_slot_shapes(&mut SlotShapeRegistry)`
  - `ensure_static_slot_shape(&mut SlotShapeRegistry, SlotShapeId)`
  - helper logic that walks registered `SlotShape::referenced_shape_ids()`
- `lp-core/lpc-engine/src/engine/engine.rs` constructs a fresh
  `SlotShapeRegistry` in `Engine::with_services`, then calls
  `register_authored_slot_shapes`, which funnels static authored ids through
  `lpc_model::slot_shapes::ensure_static_slot_shape`.
- `SlotShapeRegistry` in
  `lp-core/lpc-model/src/slot/slot_shape_registry.rs` owns:
  - `BTreeMap<SlotShapeId, SlotShapeEntry>`
  - `BTreeMap<SlotShapeId, SlotFactory>`
  - versioned snapshot/page APIs
  - dynamic registration/replacement/unregister APIs
- `SlotShape` in `lp-core/lpc-model/src/slot/slot_shape.rs` is heap-shaped:
  `Vec`, `Box`, `SlotName(String)`, `SlotMeta(String)`, dropdown option
  vectors, and nested owned values.
- `StaticSlotShape` in `lp-core/lpc-model/src/slot/slot_access.rs` currently
  exposes `fn slot_shape() -> SlotShape`, so even static descriptions are
  constructed as owned runtime values.
- `SlotAccessor::compile`, slot lookup, slot mutation, dynamic slot codecs,
  wire snapshotting, generated slot views, and engine read paths all take
  `&SlotShapeRegistry`.
- Shape read over the wire currently returns `SlotShapeRegistrySnapshot`, whose
  payload is a `BTreeMap<SlotShapeId, SlotShapeEntry>`.
- `lpc-wire` has a direct JSON writer for shape registry snapshots, but it
  still iterates the owned registry entries.

## Important Constraints

- Do not feature-gate or remove compiler/runtime behavior to save memory.
- Do not keep a firmware fallback that materializes all static shapes into owned
  heap data. That would preserve the current memory problem under a nicer API.
- Host tests may use compatibility helpers, but the engine/device path should
  not require resident static `SlotShape` maps.
- Keep dynamic/project-specific shapes possible. Shader/artifact-dependent
  shapes still need runtime registration/replacement.
- Preserve typed default factories for static shapes, but route lookup through
  generated id dispatch instead of a per-registry factory map entry.

## Proposed Assumptions

### Static catalog is authoritative for built-ins

- **Suggested answer:** Static authored shapes are part of the model crate ABI.
  The device should keep them in generated descriptors and export them to
  clients during setup without storing them in the engine registry.
- **Why:** This preserves the embedded heap win while keeping the wire/client
  setup path simple.

### Shape reads can become catalog-aware

- **Suggested answer:** Shape read should return the existing registry snapshot
  payload shape, populated with static descriptors exported from the generated
  catalog plus dynamic registry entries.
- **Why:** This keeps wire behavior understandable without adding a catalog
  fingerprint compatibility protocol.

### Static descriptors should be borrowed

- **Suggested answer:** Add a borrowed/static descriptor family instead of
  trying to make today's owned `SlotShape` const.
- **Why:** Current `SlotShape` owns heap collections and strings; a borrowed
  mirror can live in flash and be traversed without allocation.

### Codegen should own static descriptor production

- **Suggested answer:** Extend slot macros/codegen so registered static shapes
  produce generated descriptor entries and id matches.
- **Why:** The derives and codegen already know the static shape universe and
  field paths. Manual maintained tables would drift.

## Open Questions

### Should static-shape streaming remain as a dev fallback?

- **Context:** The user is open to taking a version-compat hit, but also wants
  graceful behavior for missing things.
- **Suggested answer:** Keep a catalog-aware debug/dev endpoint or response mode
  that serializes static descriptors from generated read-only data. Do not use
  it in the normal runtime sync path, and do not materialize all static shapes
  into `SlotShapeRegistry`.

### How strict should the first implementation be about hand-written shapes?

- **Context:** Derived `Slotted` types can probably produce static descriptors
  naturally. Hand-written `StaticSlotShape::slot_shape()` impls may need manual
  descriptor support or macro/helper migration.
- **Suggested answer:** Make every generated registered static shape provide a
  static descriptor before removing registration. Avoid an owned-shape fallback
  in firmware. If a shape is hard to migrate, keep that type out of the static
  catalog only when it is genuinely dynamic.

### Should generated slot views stop compiling paths through the registry in
this milestone?

- **Context:** Generated views currently call `SlotAccessor::compile(...)`,
  which requires shape lookup. Codegen already knows field names and can
  eventually emit precompiled accessor steps directly.
- **Suggested answer:** Do not make precompiled generated views the core M2
  requirement. Convert them to the new lookup abstraction first. Capture direct
  accessor generation as follow-up or an optional late phase if the catalog
  migration is stable.

## Measurement Targets

- Measure heap after `Engine::with_services` before and after removing static
  registration.
- Measure project-load profile for `examples/basic` and `examples/button-sign`
  after M1 load-only instrumentation exists.
- Track whether dynamic shapes add back meaningful heap during runtime state
  registration.
