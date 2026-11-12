# Phase 1: Slot Value Boundary

## Goal

Make the slot/value boundary explicit in `lpc-model`.

The slot tree owns addressability, versioning, sync, watch, and mutation. A
slot value is the leaf where that slot tree ends and an opaque `LpValue` tree
begins.

## Work

- Keep `SlotData::Value(Versioned<LpValue>)` as the owned dynamic representation.
- Keep `ValueSlot<T>` as the versioned typed storage wrapper.
- Keep `SlotValue` as the trait for typed Rust values that can occupy a slot
  value boundary.
- Keep `SlotValueShape` as the metadata-bearing shape at that boundary.
- Consolidate `LpValueRootId` into `SlotShapeId` so there is one shape identity
  type and one registry identity story.
- Update module docs and rustdocs so the important concepts are visible at the
  top of each file.

## Validation

- `cargo test -p lpc-model`
