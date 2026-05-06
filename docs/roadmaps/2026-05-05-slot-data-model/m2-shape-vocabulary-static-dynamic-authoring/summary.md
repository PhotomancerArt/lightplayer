# Summary

## What Was Built

- Added typed slot authoring helpers in `lpc-model`: `SlotValue<T>`, `SlotMap<K, V>`, `SlotOption<T>`, key conversion, and access traits.
- Kept `SlotData` as the owned dynamic snapshot representation and renamed dynamic map/option containers to `SlotMapDyn` and `SlotOptionDyn`.
- Added id-addressed `SlotShapeRegistry` nodes with owned/ref child edges, shape versions, and map key shape metadata.
- Made `SlotShapeId` a compact numeric id and moved static Rust-authored shape identity onto the type via `StaticSlotAccess`.
- Added an ambient state-version helper so slot containers stamp the current observable version instead of requiring call sites to thread versions through every leaf.
- Added `lpc-slot-mockup` as a temporary domain pressure harness with `model`, `source`, `engine`, `wire`, and `view` modules.
- Proved full sync and incremental patching across source defs, engine runtime state, dynamic shader params, enum switching, option clearing, and map key pruning.

## Decisions For Future Reference

#### Dynamic Snapshot Versus Typed Authoring

- **Decision:** `SlotData` is the generic owned snapshot/wire mirror; typed Rust structs expose slots through access traits.
- **Why:** This keeps source/runtime code natural while still giving wire/view code one generic representation.
- **Rejected alternatives:** Force every Rust-authored struct to allocate a `SlotRecord` mirror before traversal.
- **Revisit when:** The derive macro starts generating access impls.

#### Typed Containers Are Not Serde Payloads

- **Decision:** `SlotValue<T>`, `SlotMap<K, V>`, and `SlotOption<T>` are typed authoring/runtime helpers and do not derive serde by default.
- **Why:** Serialization belongs to `SlotData`; generic typed containers would otherwise impose serde/schema bounds on all authoring types.
- **Rejected alternatives:** Make every typed container directly serializable.
- **Revisit when:** Real source defs adopt these wrappers and need disk serialization policy.

#### Shape-Aware Client Patching

- **Decision:** The view-side mock client resolves record fields and map keys through `SlotShapeRegistry`.
- **Why:** This removes hardcoded field indexes and proves the client can apply generic slot patches from shapes alone.
- **Rejected alternatives:** Keep mock-specific field-name-to-index tables.
- **Revisit when:** Wire messages gain real slot mutation/apply semantics.

#### Shapes Live With Domain Concepts

- **Decision:** Mockup source and engine shape registration lives beside the corresponding source/engine type through `StaticSlotAccess`.
- **Why:** Opening `source/output_def.rs` should show both the authored data and the shape that makes it slot-addressable.
- **Rejected alternatives:** Keep all shape registration in one central `model/shapes.rs` file.
- **Revisit when:** A derive macro can generate these registrations from the Rust-authored structs.

#### Static Shape Identity Belongs To Types

- **Decision:** Static Rust-authored slot roots do not store `shape_id` as a value field. The type owns a `const SHAPE_ID` and registers its shape tree at startup.
- **Why:** Shape identity is metadata about the Rust-authored shape, not mutable runtime/source data. Keeping it on the type lines up with the old `lpmini2024` static shape model and keeps instance structs clean.
- **Rejected alternatives:** Store a `SlotShapeId` on every `OutputDef`, node, or state object.
- **Revisit when:** Derive support needs a generated static-id scheme.

#### Shape IDs Are Compact Registry Keys

- **Decision:** `SlotShapeId` is currently a compact `u32`, produced for static names with a const FNV-1a hash helper.
- **Why:** Embedded lookup wants a small key, while authored Rust still needs readable source-level names. The registry rejects duplicates during registration so static hash collisions fail at startup.
- **Rejected alternatives:** Carry string shape IDs through runtime lookups.
- **Revisit when:** A generated id allocator or build-time registry can replace hashing.

#### State Version Is Ambient

- **Decision:** Slot containers stamp new values, key changes, option presence, and record snapshots with `current_state_version()` by default, while retaining explicit `*_with_version` constructors for tests/import/replay.
- **Why:** Versions are ubiquitous and advance once per observable frame/epoch, so passing a mutable context through every data constructor and mutation would add noise without improving the model.
- **Rejected alternatives:** Require every source/runtime struct and nested slot container to receive a `MutationCtx`.
- **Revisit when:** `FrameId` is split into a dedicated `StateVersion` type or runtime/server ownership needs stricter scoping.

#### Map Key Shape Is Registry Data

- **Decision:** `SlotShapeNode::Map` carries `SlotMapKeyShape`.
- **Why:** The client must know whether a path segment represents a string, `i32`, or `u32` key.
- **Rejected alternatives:** Parse all patch paths as string keys.
- **Revisit when:** Slot path encoding gets a richer typed segment representation.
