# M2.1 Client-Initiated Slot Mutation Notes

## Scope Of Work

Build a focused client-initiated mutation slice for the slot model.

The goal is to make the reusable pieces real where reasonable:

- wire protocol types in `lpc-wire`,
- client-side authoritative slot mirror and pending mutation state in `lpc-view`,
- mock server/runtime mutation application in `lpc-slot-mockup`.

The mockup remains the pressure harness for server-side ownership and mutation
dispatch. It should depend on the real model/wire/view crates instead of
carrying parallel client mirror and protocol types.

Initial mutation behavior should be server-authoritative:

- the client does not optimistically mutate local `SlotData`,
- the client records pending mutations and can expose pending/error state,
- the server validates shape and data versions,
- accepted mutations update authoritative server-owned data,
- normal registry/data sync updates the client mirror,
- rejected mutations clear pending state with a reason.

## User Context And Decisions

- Client-side edits should show pending UI, such as a small spinner, until
  server confirmation arrives.
- At least one-frame latency for mutations is acceptable for now.
- The first model should use optimistic locking, not local-first merging.
- Conflicts should be rejected rather than auto-merged.
- We should make as much of this real as is reasonable; the mockup can depend on
  all real crates.
- `param_defs` remains a homogeneous map; runtime shader `params` is now a
  dynamic record.

## Current Codebase Context

### Shared Slot Model

`lp-core/lpc-model/src/slot` now owns the shared slot vocabulary:

- `SlotData`
- `SlotShape`
- `SlotShapeRegistrySnapshot`
- `SlotPath`
- `SlotShapeId`
- `SlotMapKey`
- typed authoring helpers and access traits

`SlotData` is already serde-enabled and no-std friendly, so it is suitable for
wire protocol payloads.

### Mockup Wire And View

`lp-core/lpc-slot-mockup/src/wire` currently owns mock-only protocol types and
helpers:

- `FullSync`
- `SlotPatch`
- `SlotChange::Replace`
- `full_sync`
- `collect_diff`
- debug tree printing

`lp-core/lpc-slot-mockup/src/view/mock_client.rs` currently owns the mock
client mirror:

- shape registry,
- root shape ids,
- root `SlotData`,
- full sync application,
- registry snapshot application,
- patch application.

This is the shape that should move into `lpc-view`, except debug/test-only
printing can remain in the mockup or become test utilities.

### Real Wire Crate

`lp-core/lpc-wire` is `no_std`, depends on `lpc-model`, and already owns
project-scoped request/response shapes such as `WireProjectRequest`.

The current `WireProjectRequest` only has `GetChanges`. Slot full sync,
patches, and mutation request/response types can live in either a new
`lpc-wire/src/slot` module or under `project`. A dedicated `slot` module seems
cleaner because the concepts are reusable even if currently project-scoped.

### Real View Crate

`lp-core/lpc-view` is `no_std`, depends on `lpc-model`, `lpc-wire`, and
`lpc-source`, and already owns client-side mirrors:

- `ProjectView` for legacy project response data,
- `NodeTreeView` for tree deltas,
- `ClientResourceCache` for resource summaries/payloads.

A new `slot` module can own the generic slot mirror without disturbing the
legacy project view during the pressure-harness phase.

### Mockup Runtime

`lp-core/lpc-slot-mockup/src/engine/runtime.rs` owns the fake server/runtime:

- `SlotShapeRegistry`,
- source defs,
- engine nodes,
- mutation-ish helpers such as `set_shader_param` and
  `change_shader_param_to_vec3`.

This should remain the first mutation target. Server mutation application can be
mock-specific while it proves generic wire/view semantics.

## Proposed Initial Types

### Wire Types

Suggested home: `lp-core/lpc-wire/src/slot`.

```rust
pub struct WireSlotFullSync {
    pub registry: SlotShapeRegistrySnapshot,
    pub roots: Vec<WireSlotRootSnapshot>,
}

pub struct WireSlotRootSnapshot {
    pub name: String,
    pub shape: SlotShapeId,
    pub data: SlotData,
}

pub struct WireSlotPatch {
    pub root: String,
    pub path: SlotPath,
    pub change: WireSlotChange,
}

pub enum WireSlotChange {
    Replace(SlotData),
}

pub struct WireSlotMutationId(u64);

pub struct WireSlotMutationRequest {
    pub id: WireSlotMutationId,
    pub root: String,
    pub path: SlotPath,
    pub expected_shape_version: FrameId,
    pub expected_data_version: FrameId,
    pub op: WireSlotMutationOp,
}

pub enum WireSlotMutationOp {
    SetValue(ModelValue),
}

pub struct WireSlotMutationResponse {
    pub id: WireSlotMutationId,
    pub result: WireSlotMutationResult,
}

pub enum WireSlotMutationResult {
    Accepted,
    Rejected(WireSlotMutationRejection),
}

pub enum WireSlotMutationRejection {
    ShapeConflict { current_version: FrameId },
    DataConflict { current_version: FrameId },
    WrongType,
    UnknownRoot,
    UnknownPath,
    UnsupportedTarget,
}
```

Questions remain about exact names and whether `Accepted` should carry versions.

### View Types

Suggested home: `lp-core/lpc-view/src/slot`.

```rust
pub struct SlotMirrorView {
    pub registry: SlotShapeRegistry,
    pub root_shapes: BTreeMap<String, SlotShapeId>,
    pub roots: BTreeMap<String, SlotData>,
    pub pending: BTreeMap<WireSlotMutationId, PendingSlotMutation>,
    pub errors: BTreeMap<WireSlotMutationId, WireSlotMutationRejection>,
}
```

The view should support:

- apply full sync,
- apply registry snapshot,
- apply patches,
- create/send pending mutation request from the current authoritative mirror,
- apply mutation response,
- inspect pending state by id and probably by `(root, path)`.

The view should not apply optimistic local `SlotData` changes yet.

## Open Questions

### Q1. Wire Module Location

Context:

`lpc-wire` currently groups project sync under `project`, tree sync under
`tree`, and top-level messages under `message/server`.

Suggested answer:

Add `lpc-wire/src/slot` for generic slot sync/mutation types, then re-export
from `lpc-wire/src/lib.rs`. Later project messages can embed these types where
needed.

### Q2. Mutation Id Type

Context:

Top-level `ClientMessage` already has a request id, but pending slot mutations
are useful as domain-level units inside the view. A UI may have multiple pending
mutations before they are wrapped in transport messages.

Suggested answer:

Add `WireSlotMutationId(u64)` to the slot wire types and use it in
`SlotMirrorView.pending`. It can be mapped to top-level request ids later, but
should not depend on a transport envelope.

### Q3. Shape Version Source

Context:

`SlotShapeRegistry` stores `VersionedSlotShape { changed_frame }` per root.
For nested dynamic records, only the root shape currently has a version. That
matches current registry semantics.

Suggested answer:

`expected_shape_version` should be the changed frame of the resolved root shape,
not a per-field shape version. This is coarse but correct for the current
registry.

### Q4. Data Version Source

Context:

Each slot data node has a version boundary:

- `Value.changed_frame`
- `Record.fields_changed_frame`
- `Map.keys_changed_frame`
- `Enum.variant_changed_frame`
- `Option.presence_changed_frame`

Suggested answer:

For `SetValue`, `expected_data_version` is the target value leaf version. Future
structural mutations will use the container version for the container they
change.

### Q5. Accepted Response Payload

Context:

The client mirror should only update from authoritative sync. A mutation accept
could clear pending immediately, or wait until the data patch arrives.

Suggested answer:

Return `Accepted` immediately so the view can mark request completion, but do
not mutate the mirror until patches/full sync arrive. This gives the UI a clear
"accepted, waiting for sync" state if it wants one.

### Q6. Mockup Server Mutation Dispatch

Context:

Generic slot data is a projection. Server mutation must update the owning Rust
object, not just mutate a `SlotData` snapshot.

Suggested answer:

Keep mutation dispatch explicit in `MockRuntime` for the first slice:

- support `engine.shader_node / params.exposure / SetValue(F32)`
- support at least one source value such as
  `source.shader / param_defs.exposure.label / SetValue(String)`

This keeps ownership honest and avoids inventing a broad mutation trait before
we know the shape.

## Suggested First Test Cases

- Client builds a `SetValue` request for `engine.shader_node#params.exposure`
  from the authoritative mirror, stores it as pending, server accepts, data diff
  syncs, pending clears.
- Stale data version rejects with `DataConflict`.
- Stale shape version rejects with `ShapeConflict`.
- Wrong value type rejects with `WrongType`.
- Unknown root/path rejects.
- Source mutation of `source.shader#param_defs.exposure.label` proves mutation
  is not runtime-only.

## Initial Validation Commands

```bash
cargo test -p lpc-wire
cargo test -p lpc-view
cargo test -p lpc-slot-mockup -- --nocapture --test-threads=1
cargo check -p lpc-wire --features schema-gen
git diff --check
```
