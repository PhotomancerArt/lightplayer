# ADR 2026-06-10: Shared Project Edit Vocabulary

## Status

Accepted

## Context

The registry branch needs a future UI and engine cutover path that edits authored
project artifacts, not only immediate engine memory. The existing
`WireSlotMutation*` API is intentionally narrow: it sets value leaves on runtime
slot roots like `node.<id>.def`, applies immediately, and has no overlay or
commit concept.

`lpc-node-registry` already owns overlay, effective-read, and commit mechanics,
but its local `SyncOp` mixes client-like edit operations with server-local
filesystem events. Exposing that enum on the wire would couple clients to
registry implementation details.

The naming also needed tightening before becoming protocol vocabulary:
`AssetEdit` could replace or delete any artifact body, including `.toml`
definition files, so it was broader than "asset".

## Decision

Shared authored edit nouns live in `lpc-model::edit`.

The shared vocabulary uses:

- `SlotEdit::{EnsurePresent, AssignValue, Remove}` for structured slot edits.
- `ArtifactBodyEdit` for byte-level replace/delete of artifact bodies.
- `ArtifactEdit` for artifact-path-addressed edits.
- `ProjectEditBatch` and `ProjectEditOp` for ordered client-authored commands.
- Portable command results and definition-location summaries for registry output.

`lpc-wire` defines thin wire envelopes around the model vocabulary:
`WireProjectEditRequest` and `WireProjectEditResponse`.

`lpc-node-registry` applies `ProjectEditBatch` directly and does not depend on
`lpc-wire`. Registry `SyncOp::Fs` remains server-local and is not a client wire
operation.

The legacy `WireSlotMutation*` path remains during the POC and will be removed
only after the later UI/server/engine cutover.

## Consequences

The future UI can build and serialize authored project edits without depending
on registry internals.

The registry can test wire-shaped edit behavior without becoming a protocol
crate.

The new API has a clean place to grow revisioning, idempotency, and conflict
semantics later.

`AssetEdit` remains as a registry compatibility name for now, but new shared and
wire-facing code should use `ArtifactBodyEdit`.
