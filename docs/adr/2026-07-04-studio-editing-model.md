# ADR: Studio Editing Model

- **Status:** Accepted
- **Date:** 2026-07-04
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

Milestone M1 of the Studio editing roadmap builds everything editing needs
below the UI. Before M1, the server overlay system
(`ReadOverlay`/`MutateOverlay`/`CommitOverlay`) was fully implemented
end-to-end, but nothing enforced `SlotPolicy.writable`, nothing consumed
`SlotPersistence`, `commit_overlay` cleared the whole overlay wholesale, no
overlay data reached the client in the pull, and the client had zero mutation
ops. The revision-gated read regime
(`2026-07-03-revision-gated-project-reads.md`) deliberately omitted the
overlay.

Editing raises five coupled questions this ADR answers as one decision
record: where dirty state lives, how runtime-control ("transient") edits
relate to persisted ones, where invalid writes are rejected, how overlay
state reaches the client without paying for it on every pull, and how a
field's local value survives the gap between user input and the server's
acknowledgement at real-device pull cadence (~750 ms).

## Decision

### D1 — Dirty state derives from the server overlay

A slot is dirty iff the server overlay contains an edit at its path. There is
no client-local dirty tracking; the client keeps a **mirror** of the overlay
(`ProjectSync`) and joins it into the slot DTOs at view-build time
(`SlotEditJoin`): an overlay entry at a slot's `ProjectSlotAddress` marks the
DTO `Dirty`, and `ProjectEditorView.dirty` carries
`ProjectDirtyCounts { persisted, transient }` for the Save strip.

Rationale: the server owns most state; the client owns only UI state. Dirty
state derived from the overlay is cross-client-correct and survives
reconnects for free — a reconnecting client rebuilds it from one overlay
read.

### D2 — Transient rides the overlay and never serializes; Save filters

Transient (runtime-control) edits use the same `MutateOverlay` path as
persisted edits — one mechanism, two UI chromes ("unsaved" vs "live",
`UiSlotFieldState.live` distinguishes them on the DTO). The split is enforced
at two seams:

- **The JSON slot writer omits transient fields** (`omit_record_field` in
  `lpc-model/src/slot_codec/dynamic_slot_writer.rs`). A field whose governing
  policy persistence is `Transient` is omitted with its whole subtree, and
  records left empty by omission collapse away. Because the writer is the
  only path to node-def bytes, no transient value can ever appear in a
  written def file, regardless of caller. Readers need no change: absent
  fields default from the shape on load.
- **Commit retains transient overlay entries.** `commit_overlay` rebuilds the
  overlay keeping entries whose resolved policy is transient
  (`retain_transient_edits`) instead of clearing wholesale; persisted entries
  clear as their artifacts are written. An only-transient commit changes no
  overlay content and therefore does **not** bump the overlay revision.

Revert is uniform: `RemoveSlotEdit` for one slot (for transient slots this
doubles as reset-to-authored-default), `RevertAllEdits` for the project.
`RemoveSlotEdit` is allowed regardless of writability — it only removes
pending overlay state and is needed to clean stale edits.

### D3 — Mutate-time enforcement with per-command rejection

`ProjectRegistry::mutate_batch` validates every command before applying it
and rejects invalid ones individually with a distinct
`MutationRejectionReason`: `UnknownArtifact`, `UnknownSlotPath`,
`NotWritable` (non-writable target), and `TypeMismatch` (an `AssignValue`
whose value fails `lp_value_matches_type` against the leaf type). Batches
stay per-command and non-transactional: the rest of the batch proceeds, and
each rejection reaches the client on its `MutationCmdId`.

Policy is resolved **shape-only** (`lpc-model/src/slot/slot_policy_lookup.rs`),
so edits validate at paths where no data exists yet (missing map entries,
inactive enum variants). The inheritance rule: policy is declared per record
field, and the innermost field on the walk with a non-default declared policy
governs its whole subtree; map-key/option/variant segments pass it through;
an explicit `writable_persisted` is indistinguishable from undeclared and
inherits. Type checking applies to value leaves only — composite-edit
semantics are M3's.

### D4 — Overlay joins the revision-gated read regime as a ride-along

`ProjectRuntimeStatus` gains `overlay_changed_at`, stamped by the engine
stream source (`EngineProjectReadSource::stream_query_events` reads
`registry.overlay().changed_at()` — the source already holds the registry,
so every read path reports the true revision with no stale window; zero
means never mutated). The client compares it against its mirror's revision
and issues a full `ReadOverlay` only on change (`!=`, self-correcting under
monotonic revisions). There is **no `since` parameter** on overlay reads —
overlays are small, so fetch-full-on-change beats a delta protocol in
complexity. A quiet-but-dirty project issues no `ReadOverlay` and transfers
no overlay payload (contract-tested).

Mutation and commit responses carry the resulting overlay revision
(`WireOverlayMutationResponse.overlay_revision`,
`WireOverlayCommitResponse.overlay_revision`,
`WireOverlayReadResponse.revision`), so a client that mutates applies its own
accepted commands to the mirror and stamps the acked revision
(`ProjectSync::apply_acked_edits`) without a follow-up fetch. The one
exception is Save: because an only-transient commit does not bump the
revision (D2), `SaveOverlay` re-syncs via a full overlay read rather than
trusting the ack alone.

### D5 — Path-keyed edit buffer with ack-based release; per-address coalescing

Field components are stateless views; the value a user typed lives in a
buffer in `ProjectController`, keyed by `ProjectSlotAddress`
(`PendingEdit`, `lpa-studio-core/src/app/project/slot/pending_edit.rs`):

```text
(field input) ──► Pending { value }            # op queued/coalescing
op sends       ──► InFlight { value, cmd_id }
ack accepted   ──► (entry removed; mirror updated via
                    ProjectSync::apply_acked_edits — the slot now reads
                    dirty from the overlay mirror)
ack rejected   ──► Failed { value, reason }    # feeds UiSlotFieldState
                                               # `invalid`; cleared on the
                                               # next edit or an explicit
                                               # revert
op error/timeout ─► Failed { value, transport reason }
```

While an entry exists, the DTO shows the buffered value (shadowing the
synced value) and the phase maps to the dirty affordance:
`Pending`/`InFlight` → `Saving`, `Failed` → `Error` + `invalid` reason. On
accept the shadow hands off to the overlay mirror, which keeps supplying the
assigned value until the next project read catches up — so there is no
rubber-band window either before the ack or between the ack and the next
pull.

Input floods are absorbed in the actor: consecutive queued
`SlotEditOp::SetValue`s for the same address collapse latest-wins in the
batch planner (`studio_actor.rs::push_action_coalesced`); any other action is
a barrier, preserving order. The ops are `SlotEditOp { SetValue, Revert }`
per slot plus project-level `ProjectOp::{SaveOverlay, RevertAllEdits}`, all
`ActionClass::Foreground` with the 6 s editor quiet-gap deadline
(`PROJECT_EDITOR_ACTION_DEADLINE`).

## Consequences

- Reconnect and multi-client dirty state are free: one overlay read rebuilds
  everything; no dirty-state reconciliation protocol exists to get wrong.
- Transient safety is structural, not procedural — the writer-level omission
  means no future call site can accidentally persist a runtime control.
- New wire surface stays lean (one `Revision` on the runtime status, one per
  overlay response), and firmware carries no new parsing burden.
- A rejected edit is visible on the exact field that caused it, with the
  user's value preserved for correction.
- The overlay revision is content-hash-like only in the sense of "changed":
  an only-transient commit is invisible to gating, which is why Save must
  re-read (D4). Anything else that mutates-without-bumping would be a bug.
- The e2e harness (`studio_edit_e2e_tests.rs`) drives a real in-process
  `LpServer` through edit → coalesce → save → revert-all, so the loop above
  is regression-tested end to end.

## Alternatives Considered

- **Client-local dirty tracking.** Rejected: it double-books state the
  server already owns, breaks cross-client visibility, and needs
  reconciliation on reconnect.
- **`since`-gated / per-item overlay deltas.** Rejected for now: overlays
  are small, and fetch-full-on-change needs no delta bookkeeping. Revisit
  per-item gating if overlays grow (Follow-ups).
- **Extending project-read events to carry the overlay.** Rejected (roadmap
  D7): a `ReadOverlay` ride-along gated by revision keeps the read-event
  contract untouched.
- **A separate channel/protocol for transient runtime controls.** Rejected:
  one mutate path plus policy-driven filtering gives uniform revert,
  uniform rejection, and one buffer implementation.
- **Commit-time transient filtering only (no writer omission).** Rejected:
  only the writer seam guarantees "never serializes" for every caller.
- **Releasing the buffered value on blur or on the next pull.** Rejected
  (roadmap D6): at ~750 ms device pull cadence both rubber-band; only the
  mutation ack proves the server has the value.
- **Transactional mutation batches.** Rejected (roadmap Q2): per-command
  results already exist on the wire, and partial acceptance matches the
  per-field editing UX.
- **Overlay persistence across restarts.** Rejected (roadmap D4): a
  device-crashing edit must not crash-loop.

## Follow-ups

Per the deferred-decision convention, these are indexed in
`docs/adr/README.md`.

- **(a) Per-item overlay gating.** Fetch-full-on-change assumes small
  overlays. **Revisit when** measured overlay fetch cost matters (large
  overlays or chatty editing sessions on slow links).
- **(b) Save-panel diff DTOs.** M1's dirty join provides counts only; a
  "what changed" panel needs before/after value DTOs. **Revisit in** roadmap
  M3 (Save panel with change count).
- **(c) Composite edit semantics.** Type enforcement covers value leaves;
  map entry add/remove, option some/none, and enum variant switching have
  real design decisions. **Revisit in** roadmap M3 — extend this ADR or add
  a note if M3 sets precedent.
- **(d) Singular `ProjectRegistry::mutate` is unvalidated.** Only
  `mutate_batch` (the wire surface) enforces policy and type checks;
  `mutate` applies unconditionally. Acceptable today because no wire path
  reaches it. **Revisit when** any new caller of `mutate` appears — route it
  through the same validation or delete it.
- **(e) BLOCKING for M2 — revert regresses effective-def revisions
  (pre-existing server-side bug).** Clearing overlay edits (revert) moves
  effective-def revisions *backwards*, so a `since`-gated project read skips
  the reverted values: a connected client shows stale values after
  `RevertAllEdits` until a full (`since = 0`) resync, while the runtime
  itself reverts correctly. The e2e test works around it by reconnecting
  (`studio_edit_e2e_tests.rs`). **Must be fixed before roadmap M2 ships
  revert UX** — that milestone's revert affordance depends on the client
  seeing the reverted value without a reconnect.
