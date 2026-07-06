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

### D6 — Minimal-diff overlay normalization (added 2026-07-05)

The overlay is a **minimal diff against saved state**: a `PutSlotEdit`
assigning a value equal to the base (unoverlaid) value is not stored — the
server normalizes it to removing any existing entry at that path
(`normalize_assign_to_base` in `ProjectRegistry`; applies to both
`mutate_batch` and the singular `mutate`). "Edited, then changed back"
therefore reads Clean, converging on the same state as an explicit Revert.
UX driver: choice-type values — select a diagnostic mode, use it, set it
back to "Off"; the edited highlight must disappear.

What this deliberately gives up: an overlay entry equal to the current base
no longer *pins* the value against future base changes. For an editing
surface, "I changed it back" means "no change" — pinning was an accident of
the storage model, not a feature anyone asked for.

Two load-bearing details:

- **Ack fidelity.** The per-command result effect distinguishes
  `OverlayChanged` from `NormalizedToRemoval`, and the client mirror applies
  the **effect**, not the sent command
  (`ProjectSync::apply_acked_edits` → `effective_mutation`). Without this
  the mirror would show dirty while the server overlay is clean — and since
  an elided edit may not advance the overlay revision, no corrective fetch
  would ever fire.
- **Exact equality.** Comparison is exact `LpValue` equality: a near-miss
  float (1.0000001 vs 1.0) remains an edit. Predictable, and correct for
  step-quantized controls; revisit only if a real near-miss annoyance
  appears.

### D7 — Composite edit semantics: gestures are the wire ops (added 2026-07-06)

M3 (generic editors) resolves follow-up (c). Four coupled decisions:

- **Gestures ARE the wire ops; the server owns all defaults.** Map entry add
  = `EnsurePresent map[key]`; entry remove = `Remove map[key]`; option on =
  `EnsurePresent opt.some`; option off = `Remove opt`; enum variant switch =
  `EnsurePresent enum.variant` (raw declared ident, verbatim). The client
  never constructs composite values — `EnsurePresent` creates map entries /
  option `Some` / variant payloads server-side with factory defaults
  (`slot_factory.rs`), idempotent when already present. Client ops mirror
  the vocabulary (`SlotEditOp::{EnsurePresent, RemoveValue}` in
  `lpa-studio-core`); structural ops never coalesce and are coalescing
  barriers, preserving order against `SetValue` floods.
- **Structural normalization is base-relative**, extending D6's minimal-diff
  rule to structure (`normalize_edit_to_base`): an `EnsurePresent` that is a
  no-op against the **base** (saved) def — the map key present, the option
  `Some`, the enum already on that variant — and a `Remove` at a path the
  base does not contain both normalize to removing any overlay entry at that
  path (`NormalizedToRemoval`). Add-then-remove of a map entry therefore
  cancels to a clean overlay: no phantom dirty. Presence is resolved by
  `base_slot_presence` (a `lookup_slot_data` walk over the re-parsed base
  def); an unreadable base never normalizes in either direction.
- **Prefix-aware dirty join.** A composite slot (record/map/option/enum) is
  dirty when any overlay/buffer edit path is at or strictly under its path.
  This is what makes a *removed* map entry visible: its row is gone from
  the effective def, but the parent map row reads dirty. Counting stays
  per **edit entry**, never per row (`SlotEditJoin::entries` is the single
  enumeration feeding `DirtySummary` and the save panel's `UiPendingEdit`
  list, so counts and list agree by construction); prefix-dirty ancestors
  are display state, never additional counts. Only rows with an edit entry
  at their **own** path (`UiConfigSlot.edit_entry_address`) offer a
  row-level Revert — a prefix-only-dirty composite does not, since a revert
  at its own path would remove nothing; its entries revert individually
  from the save panel.
- **One address per value leaf.** Vector/matrix component inputs
  read-modify-write the whole `LpValue` at the leaf's single address;
  per-address latest-wins coalescing absorbs rapid multi-component edits.
  No per-component addressing exists. Relatedly, a composite-target
  `AssignValue` is rejected up front with the distinct
  `MutationRejectionReason::NotAValueLeaf` (wire `not_a_value_leaf`) rather
  than a misleading `TypeMismatch` — whole-composite assignment is not part
  of the model; gestures compose from `EnsurePresent`/`Remove` plus leaf
  assigns.

**Known base-relative edge (accepted).** Normalization compares against
base, not against the effective def, and the overlay stores one op per
path. Toggling a base-present option **off** stores `Remove opt` (a real
diff); toggling it back **on** dispatches `EnsurePresent opt.some`, which
normalizes away against the base (`.some` is base-present) *at a different
path* — the stored `Remove opt` survives, so off-then-on does **not**
restore the value or any prior interior edits. The same shape applies to
switching an enum back to its base variant while a variant switch is
pending. Recovery is explicit and always available: **Revert** on the row
(the stored entry is at the row's own path, so the row-level revert is
offered) or the save panel's per-entry revert. This is the deliberate
consequence of "the overlay is a minimal diff against saved state" — the
gesture pair is not an undo stack. The UI keeps honest through it: the
option toggle is a controlled control whose visual state only follows the
DTO's effective presence, so a normalized no-op gesture leaves the toggle
visibly unchanged rather than desynced.

**Known limitation (recorded, no code).** An optional-*wrapped* map or enum
(`Option<Map<…>>`, `Option<Enum<…>>`) would flatten its interior body into
the option row without projecting the interior's gesture facts
(`ui_composite` reads the option's own body), losing the add-entry/variant
affordances. No such shape exists in any demo project today; revisit when
one appears (tracked in the M3 plan notes as future work).

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
- **(b) Save-panel diff DTOs.** Partially superseded by M3: the save panel
  now lists labeled per-entry changes (`UiPendingEdit` — node label, slot
  path, op description, **current** value display string, phase, per-entry
  revert), built from the same join enumeration as the dirty counts. What
  remains deferred is before/after value DTOs (old value alongside new).
  **Revisit when** display strings prove insufficient — e.g. a real "what
  was it before?" need in the save panel.
- **(c) ~~Composite edit semantics.~~ Resolved 2026-07-06** by M3 — see
  **D7** above (gestures-are-wire-ops, base-relative structural
  normalization, prefix-aware dirty, one-address-per-leaf,
  `NotAValueLeaf`).
- **(f) Alternative dirty modes.** Minimal-diff normalization (D6) fixes
  the dirty semantics to "differs from saved". If a use case for
  touched-mode tracking or deliberate value pinning appears (e.g. holding a
  slot against concurrent base changes), it would be an explicit per-edit
  mode, not a return to the accidental pin. **Revisit when** such a use
  case actually shows up.
- **(d) Singular `ProjectRegistry::mutate` is unvalidated.** Only
  `mutate_batch` (the wire surface) enforces policy and type checks;
  `mutate` applies unconditionally. Acceptable today because no wire path
  reaches it. **Revisit when** any new caller of `mutate` appears — route it
  through the same validation or delete it.
- **(e) ~~BLOCKING for M2 — revert regresses effective-def revisions
  (pre-existing server-side bug).~~ Fixed 2026-07-05.** Clearing overlay
  edits (revert) moved effective-def revisions *backwards*, so a
  `since`-gated project read skipped the reverted values: a connected client
  showed stale values after a revert until a full (`since = 0`) resync,
  while the runtime itself reverted correctly. Fixed by keeping revision
  stamps monotonic: when an operation removes an artifact's overlay coverage
  (`RemoveSlotEdit` emptying it, `Clear`, commit dropping persisted
  entries), the registry stamps that artifact at the current frame
  (`ProjectRegistry::stamp_artifacts_leaving_overlay` →
  `ArtifactStore::mark_content_changed`), so the reverted defs' effective
  revisions advance and gated reads deliver them. Guarded by the engine
  contract test `reverted_slot_edit_resends_def_root` (M5 G6a suite),
  registry-level revision tests, and the e2e tests in
  `studio_edit_e2e_tests.rs`, which no longer reconnect after revert.
