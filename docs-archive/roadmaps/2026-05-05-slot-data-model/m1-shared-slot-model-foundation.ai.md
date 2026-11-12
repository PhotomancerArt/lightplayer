# Milestone 1: Shared Slot Model Foundation

## Title And Goal

Establish the foundational slot data model in `lpc-model`.

## Suggested Plan Location

`docs/roadmaps/2026-05-05-slot-data-model/m1-shared-slot-model-foundation/`

## Scope

In scope:

- Add `SlotPath`.
- Update `SlotRef` to use `SlotPath`.
- Add `SlotTree`.
- Add foundational `SlotData`, `SlotRecord`, `SlotShape`, `SlotShapeId`, and
  `SlotRegistry` types.
- Add small metadata types needed by `SlotShape`.
- Add `ModelValue::Resource(ResourceRef)` and the matching type/shape support.
- Add focused tests and rustdocs for the invariants.

Out of scope:

- Applying the model to real node defs.
- Runtime resolver/binding rewrites.
- Generic wire sync.
- Derive macros.
- Server-side mutation APIs.

## Key Decisions

- Core model types live in `lpc-model`.
- Shape registry exists from the beginning.
- A registered `SlotShapeId` owns one complete `SlotShape` tree.
- No internal shape-id references inside a registered shape tree yet.
- `SlotData`, not `SyncData`, is the shared data name.
- `ModelValue` remains the leaf payload name for now.

## Deliverables

- New or expanded modules under `lp-core/lpc-model/src/slot/`.
- `ModelValue::Resource(ResourceRef)` support.
- Tests for slot paths, slot refs, registry lookup, shape/data compatibility,
  and resource value round trips.
- Updated `lpc-model` exports and docs.

## Dependencies

- Produced/consumed slot runtime cleanup is complete.

## Execution Strategy

Full plan. This milestone establishes core vocabulary and will affect shared
types used by multiple crates.

