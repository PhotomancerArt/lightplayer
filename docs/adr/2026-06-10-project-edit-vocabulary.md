# ADR 2026-06-10: Canonical Project Overlay Vocabulary

## Status

Accepted

## Context

The registry branch needs a future UI and engine cutover path that edits
authored project artifacts, not only immediate engine memory. At the time this
ADR was written, the temporary `WireSlotMutation*` API was intentionally
narrow: it set value leaves on runtime slot roots like `node.<id>.def`, applied
immediately, and had no overlay or commit concept. That temporary API was
removed during the M4 wire/server cutover.

The first registry wire POC proved useful behavior, but it duplicated the same
ideas across layers with command-shaped types such as `ArtifactEdit`,
`ArtifactEditOp`, and `ProjectEditBatch`, while the registry still had its own
`ArtifactEdits` / `AssetEdit` overlay model.

We want the model layer to own the durable vocabulary, the wire layer to wrap it
in message envelopes, and the registry to apply it.

## Decision

`lpc-model::edit` owns the canonical project overlay model:

- `ProjectOverlay`: the full current pending edit set for a project.
- `ArtifactOverlay`: either a structured `SlotOverlay` or one
  `ArtifactBodyEdit`.
- `SlotOverlay`: canonical map from `SlotPath` to `SlotEditOp`.
- `SlotEdit`: path-bearing slot edit.
- `SlotEditOp`: path-free operation, one of `EnsurePresent`,
  `AssignValue(LpValue)`, or `Remove`.
- `ArtifactBodyEdit`: byte-level artifact body `ReplaceBody(Vec<u8>)` or
  `Delete`.
- `OverlayMutation`: ordered edits to the overlay itself.

The overlay represents current pending intent, not edit history. Multiple
mutations targeting the same path are coalesced into one overlay entry.
Ancestor/descendant conflicts are normalized in model code. Artifact body edits
and slot overlays are mutually exclusive for a given artifact.

`lpc-wire` defines thin wire envelopes around model types:

- read overlay request/response returns a full `ProjectOverlay`;
- mutate overlay request/response applies an ordered `OverlayMutationBatch`;
- commit overlay request/response returns a portable `ProjectCommitSummary`.

`lpc-registry` stores and applies `ProjectOverlay`. It owns path validation,
slot application, effective inventory derivation, filesystem writes/deletes, and
commit. It does not define a second overlay model and does not depend on
`lpc-wire` in library code.

The legacy `WireSlotMutation*` path was allowed during the POC, but M4 removed
it in favor of overlay project commands.

## Consequences

The future UI can read and mirror one canonical pending overlay instead of
reconstructing pending state from command history.

Wire schemas stay message-shaped and avoid copying model concepts.

Registry tests can exercise wire-shaped behavior without coupling registry
library code to the protocol crate.

Overlay application order is deterministic and derived from the canonical
`SlotOverlay` map; user mutation order affects coalescing, not the persisted
overlay representation.

Revisioning, idempotency, conflict semantics, and backward compatibility remain
future wire/API work.
