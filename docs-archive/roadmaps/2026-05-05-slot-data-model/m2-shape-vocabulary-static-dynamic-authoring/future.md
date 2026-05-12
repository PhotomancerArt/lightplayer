## Generic Versioned Registry

- **Idea:** Explore a generic `VersionedRegistry<K, V>` / `RegistryAccess`
  abstraction for id-keyed stores with key-set versions and per-entry versions.
- **Why not now:** Render products, runtime buffers, client resource cache, and
  slot shapes have similar maps but different payload/materialization semantics.
  M2 should only solve the slot-shape registry unless duplication becomes
  concrete.
- **Useful context:** `lp-core/lpc-engine/src/render_product/render_product_store.rs`,
  `lp-core/lpc-engine/src/runtime_buffer/runtime_buffer_store.rs`,
  `lp-core/lpc-view/src/project/resource_cache.rs`,
  `lp-core/lpc-model/src/slot/slot_registry.rs`.

## Generated Static Shape IDs

- **Idea:** Replace handwritten `SlotShapeId::from_static_name(...)` constants
  with derive-generated ids, ideally backed by a build-time registry or another
  deterministic allocation scheme.
- **Why not now:** A const hash is enough for the pressure harness and the
  registry detects duplicate ids during startup registration. The derive macro
  milestone is the right place to decide whether ids remain hash-derived or move
  to generated numeric allocation.
- **Useful context:** The older `lpmini2024` data model generated static shape
  definitions at the Rust type boundary and did not store shape identity on each
  runtime value.

## Dedicated StateVersion Type

- **Idea:** Split synchronized state versioning from `FrameId` once the access
  model moves beyond the mockup.
- **Why not now:** `FrameId` has enough resolution for the current model and the
  pressure harness only needs an ambient monotonically increasing epoch. The
  important design decision is that version stamping is ambient and advanced by
  runtime orchestration, not passed through every nested mutation.
- **Useful context:** The current helper lives in `lpc-model` even though only
  server/runtime code should normally advance it. That is acceptable for the
  mockup, but real integration may want a tighter ownership boundary.

## Message API For Slot Mutation

- **Idea:** Add server/client messages that mutate artifacts and runtime-owned
  slot data at slot paths without rewriting whole files.
- **Why not now:** Depends on M2's access/version model and later source/runtime
  application milestones.
- **Useful context:** Artifact mutation is likely the bridge from UI editing to
  project files and runtime params.
