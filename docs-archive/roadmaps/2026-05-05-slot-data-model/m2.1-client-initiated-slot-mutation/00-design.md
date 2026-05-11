# M2.1 Client-Initiated Slot Mutation Design

## Scope Of Work

Build a server-authoritative client mutation slice for generic slot data.

In scope:

- real slot sync and mutation protocol types in `lpc-wire`,
- real generic slot mirror and pending mutation state in `lpc-view`,
- migration of the mockup client to the real view type,
- mockup server mutation dispatch for a small set of explicit targets,
- optimistic locking using shape and data versions,
- tests for accepted mutations and rejection cases.

Out of scope:

- optimistic local value updates,
- real `lpc-engine` / `lpc-source` mutation integration,
- broad generic mutation traits,
- structural mutation operations beyond the behavior needed by the harness,
- transport wiring into `ClientMessage` / `ServerMsgBody`.

## File Structure

```text
lp-core/lpc-wire/src/
  slot/
    mod.rs
    sync.rs
    mutation.rs

lp-core/lpc-view/src/
  slot/
    mod.rs
    mirror.rs
    pending.rs
    apply.rs

lp-core/lpc-slot-mockup/src/
  wire/
    debug.rs
    diff.rs
    path.rs
    snapshot.rs
  engine/
    runtime.rs
  tests/
    mutation.rs
```

`lpc-slot-mockup/src/wire` keeps traversal, snapshot, diff collection, and debug
helpers because those still operate directly on mock runtime roots. Its protocol
payloads should be replaced by `lpc-wire` slot types.

`lpc-slot-mockup/src/view` should disappear or become a small re-export if
needed. The client mirror should come from `lpc-view`.

## Architecture Summary

```text
Client UI / test
  asks SlotMirrorView to prepare SetValue mutation
      |
      v
lpc-view
  records PendingSlotMutation
  returns WireSlotMutationRequest
      |
      v
mock server/runtime
  validates root/path/shape version/data version/type
  applies mutation to owned Rust object
  returns WireSlotMutationResponse
      |
      v
lpc-view
  applies response, clears or records error
  authoritative data remains unchanged until sync
      |
      v
mock wire diff/full sync
  sends WireSlotPatch / WireSlotFullSync
      |
      v
lpc-view
  applies registry snapshot and patches to mirror
```

The source of truth remains server-owned Rust data. `SlotData` in the client is
only an authoritative mirror of what the server last synced.

## Main Components

### `lpc-wire::slot`

Owns serde-friendly protocol payloads:

- `WireSlotFullSync`
- `WireSlotRootSnapshot`
- `WireSlotPatch`
- `WireSlotChange::Replace`
- `WireSlotMutationId`
- `WireSlotMutationRequest`
- `WireSlotMutationOp::SetValue`
- `WireSlotMutationResponse`
- `WireSlotMutationResult::{Accepted, Rejected}`
- `WireSlotMutationRejection`

These types use shared `lpc-model` slot primitives directly.

### `lpc-view::slot::SlotMirrorView`

Owns client-side generic slot state:

- shape registry,
- root shape ids,
- root `SlotData`,
- pending mutations,
- mutation errors.

It supports:

- `apply_full_sync`,
- `apply_registry_snapshot`,
- `apply_patches`,
- `prepare_set_value`,
- `apply_mutation_response`,
- pending/error inspection.

It does not optimistically mutate `roots`.

### Mockup Server Mutation Dispatch

`MockRuntime` adds an explicit mutation apply method:

```rust
fn apply_slot_mutation(
    &mut self,
    request: WireSlotMutationRequest,
) -> WireSlotMutationResponse
```

The first implementation validates generically where useful, then dispatches
explicitly to owning mock structures.

Initial accepted targets:

- `engine.shader_node`, path `params.exposure`, `SetValue(F32)`.
- `source.shader`, path `param_defs.exposure.label`, `SetValue(String)`.

Rejection coverage:

- unknown root,
- unknown path,
- stale shape version,
- stale data version,
- wrong value type,
- unsupported target.

## Version Semantics

`expected_shape_version` is compared against the changed frame of the resolved
root shape in `SlotShapeRegistry`.

`expected_data_version` for `SetValue` is compared against the target value
leaf's `changed_frame`.

Future structural operations should compare against the relevant container
version:

- record field add/remove/reorder: `fields_changed_frame`,
- map key add/remove: `keys_changed_frame`,
- enum variant switch: `variant_changed_frame`,
- option presence change: `presence_changed_frame`.

## Mutation Lifecycle

1. Client prepares mutation from current mirror.
2. Client records pending mutation by `WireSlotMutationId`.
3. Server validates and applies or rejects.
4. Client applies response:
   - accepted: mark request accepted/complete but keep mirror untouched,
   - rejected: remove pending and store rejection.
5. Client applies normal registry/data sync from server.
6. The authoritative mirror changes only in step 5.

## Notes For Future Work

- Transport messages can embed slot mutation requests/responses after this model
  is proven.
- A real server mutation trait may emerge after several source/runtime owners
  are integrated.
- Optimistic local edits can be added later using the same mutation id and
  rejection machinery.
