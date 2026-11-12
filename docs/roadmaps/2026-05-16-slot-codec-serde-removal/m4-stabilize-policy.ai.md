# Milestone 4: Stabilize SlotCodec Serialization Policy

## Title And Goal

Stabilize the post-M3 serialization architecture: retire obsolete source-era
crate boundaries, keep SlotCodec as the firmware path for slot-authored domain
data, and keep Serde where measurement shows it is a useful flat
infrastructure cost.

## Archived Earlier Direction

The earlier "remove Serde from `lpc-model` wholesale" M4 plan was abandoned
after firmware bloat measurements showed the SlotCodec migration had already
reduced the domain cost while `serde_core` remained a flat, acceptable
infrastructure cost. That plan is archived under `docs/roadmaps-old/`.

## Scope

In scope:

- retire `lpc-source` from the active workspace
- keep `lp-vis/lpv-model` on disk as disabled reference material
- move tiny still-useful source-era surfaces, such as `ArtifactReadRoot`, into
  their new owner crate
- keep `serde` available for `lpc-model` and `lpc-wire` where it remains useful
  for protocol/tooling surfaces
- document the firmware bloat measurement that changed the M4 direction
- scrub active docs so they describe the measured policy instead of wholesale
  Serde removal

Out of scope:

- removing Serde from `lpc-model` or `lpc-wire` without a fresh bloat reason
- changing schema versioning policy
- project-builder/authored project writing migration; that should land as its
  own step before the final serde deletion
- broad `NodeDef` API reshaping; keep only final naming polish if needed

## Key Decisions

- Slot-authored project/node definitions use SlotCodec on firmware.
- Serde remains acceptable for small wire envelopes, tests, host tooling, and
  other non-slot shells.
- If future measurements show `SlotShape`, `SlotData`, or `LpValue` serde
  serialization is too expensive, replace that specific path instead of
  deleting Serde wholesale.

## Deliverables

- `lpc-source` removed from active dependency graph.
- `lp-vis/lpv-model` disabled from workspace membership and documented as
  reference material.
- Firmware bloat report recorded.
- Docs describe the measured SlotCodec/Serde policy.

## Dependencies

- M2 message paths switched.
- M3 definition loading switched.

## Execution Strategy

Focused cleanup. This is a stabilization milestone, not a second serialization
rewrite.
