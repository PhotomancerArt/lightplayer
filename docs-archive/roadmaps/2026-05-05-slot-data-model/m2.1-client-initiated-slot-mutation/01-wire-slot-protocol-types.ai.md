# Phase 1: Wire Slot Protocol Types

## Scope Of Phase

Add real generic slot sync and mutation protocol types to `lpc-wire`.

In scope:

- `lpc-wire/src/slot` module.
- Full sync/root snapshot/patch/change wire structs currently duplicated in the mockup.
- Slot mutation id, request, op, response, result, and rejection types.
- Serde and schema-gated derives consistent with `lpc-wire`.
- Round-trip tests for sync and mutation payloads.

Out of scope:

- Transport integration into `ClientMessage` or `ServerMsgBody`.
- Server-side mutation application.
- Client mirror state.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep `no_std + alloc` compatibility.
- Put tests at the bottom of each file.
- Do not add temporary TODOs.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-wire/src/lib.rs`
- new `lp-core/lpc-wire/src/slot/mod.rs`
- new `lp-core/lpc-wire/src/slot/sync.rs`
- new `lp-core/lpc-wire/src/slot/mutation.rs`
- current mock-only reference: `lp-core/lpc-slot-mockup/src/wire/types.rs`

Expected wire types:

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

Use shared `lpc_model` types directly:

- `SlotData`
- `SlotPath`
- `SlotShapeId`
- `SlotShapeRegistrySnapshot`
- `FrameId`
- `ModelValue`

## Validate

```bash
cargo test -p lpc-wire slot
cargo check -p lpc-wire --features schema-gen
```
