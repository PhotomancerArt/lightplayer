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
ack accepted,
  overlay stored ─► (entry removed; mirror updated via
                    ProjectSync::apply_acked_edits — the slot now reads
                    dirty from the overlay mirror)
ack accepted,
  normalized to a removal that changed the overlay
               ──► AwaitingRefresh             # the mirror holds nothing at
                                               # the path; the entry keeps
                                               # its shadow until the next
                                               # project read is applied
ack rejected   ──► Failed { value, reason }    # feeds UiSlotFieldState
                                               # `invalid`; cleared on the
                                               # next edit or an explicit
                                               # revert
op error/timeout ─► Failed { value, transport reason }
```

While an entry exists, the DTO shows the buffered value (shadowing the
synced value) and the phase maps to the dirty affordance:
`Pending`/`InFlight`/`AwaitingRefresh` → `Saving`, `Failed` → `Error` +
`invalid` reason. On accept the shadow hands off to the overlay mirror,
which keeps supplying the assigned value until the next project read catches
up — so there is no rubber-band window either before the ack or between the
ack and the next pull.

The `AwaitingRefresh` phase (added 2026-07-06) closes the one gap in that
hand-off: an accepted ack whose effect is `NormalizedToRemoval { changed:
true }` (D6/D7 — a value set back to its base, an add-then-remove gesture
cancelling) leaves the mirror with **no** entry to hand off to, while the
synced view still holds the superseded effective value until the next gated
read delivers the reverted def. Releasing the entry at the ack would fall
back to that stale value for one pull cycle — visible jitter on exactly the
"changed it back" gesture D6 exists for. Instead the entry parks as
`AwaitingRefresh` (keeping its shadow and the `Saving` affordance, counted
like an in-flight edit) and `ProjectController::apply_project_view` releases
it when the next project read is applied; ops and sync runs are serialized
on the actor and revision stamps are monotonic, so the first read applied
after the ack already carries the post-normalization values. A `changed:
false` normalization altered nothing and releases immediately.

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
  are display state, never additional counts. Only rows with an **own**
  edit entry (`UiConfigSlot.edit_entry_address`) offer a row-level Revert:
  the row's own path, plus two ownership extensions for entries that are
  semantically the row's own gesture but live one segment deeper — a
  present option row owns an entry at its interior `.some` address (its
  value renders inline on the row), and an enum row owns an entry at a
  declared variant child path (`enum.Variant` — where the variant-switch
  gesture stores; the active variant is checked first, the other declared
  variants cover the ack-to-refresh window). A prefix-only-dirty composite
  offers no row revert, since a revert at its own path would remove
  nothing; its entries revert individually from the save panel.
- **One address per value leaf.** Vector/matrix component inputs
  read-modify-write the whole `LpValue` at the leaf's single address;
  per-address latest-wins coalescing absorbs rapid multi-component edits.
  No per-component addressing exists. Relatedly, a composite-target
  `AssignValue` is rejected up front with the distinct
  `MutationRejectionReason::NotAValueLeaf` (wire `not_a_value_leaf`) rather
  than a misleading `TypeMismatch` — whole-composite assignment is not part
  of the model; gestures compose from `EnsurePresent`/`Remove` plus leaf
  assigns.

**Map key moves (added 2026-07-06, M3 gate feedback).** Keys are path
segments, so re-keying a map entry is its own mutation, not a value edit:
`MutationOp::MoveSlotEntry { artifact, from, to }` (sibling entry paths,
canonical path-string encoding — no new key wire type). The server
**materializes** the move into the existing edit vocabulary
(`lpc-registry/src/overlay/move_slot_entry.rs`): `EnsurePresent to`, then
edits wherever the moved **effective** value diverges from a *simulated
future target entry* (base + current overlay + the stored form of the
leading ensure — so a base-absent key diffs against factory defaults and a
base-present key with a pending remove diffs against the base entry), then
`Remove from`. Composite values survive: a non-default variant re-emits its
`EnsurePresent to.Variant` selection, nested map entries re-add, diverged
leaves re-assign. Every synthesized edit passes through the same
base-relative normalization (D6/D7), so a move that reconstructs base state
at `to` ends with a minimal (possibly empty) overlay; when the trailing
`Remove from` normalizes away, edits stranded strictly under `from` are
removed explicitly. Ack fidelity extends the effect vocabulary:
`MutationEffect::Materialized { edits: Vec<StoredSlotEdit>, changed }`
lists the stored per-path edits (`Put { edit }` / `Removed { path }`) in
application order, and the mirror replays them verbatim
(`ProjectSync::apply_acked_edits` → `effective_mutations`). Rejections:
absent source / non-map paths → `unknown_slot_path`; occupied target →
the dedicated `target_occupied`, so the key editor can surface "key already
in use" on the row. Client op: `SlotEditOp::MoveEntry { address(map),
from_key, to_key }` — structural (never coalesces, coalescing barrier),
staged at the map's own address, released by the ack like any gesture.

**Option off-then-on (fixed 2026-07-06).** Normalization compares against
base and the overlay stores one op per path, which used to leave a
counteracting entry behind: toggling a base-present option **off** stores
`Remove opt` (a real diff); toggling it back **on** dispatches
`EnsurePresent opt.some`, which normalizes away against the base (`.some`
is base-present) *at a different path* — the stored `Remove opt` survived
and the toggle-on click did nothing. The **counteracting-entry rule**
closes this: a structural `EnsurePresent` that normalizes away also clears
the overlay subtree at its *effective scope*
(`ProjectRegistry::structural_ensure_scope` — the parent option path when
the terminal segment is an option's `some` per the shape walk, the ensure
path itself otherwise, e.g. a map-entry re-add cancelling a stored
`Remove map[k]`). The sweep covers both the counteracting `Remove` and any
stale edits under it, and the registry reports the cleared entries through
`MutationEffect::Materialized` so ack-mirroring clients follow without a
fetch. Re-enabling a base-present option therefore *is* a clean cancel of
a pending toggle-off. The option toggle stays a controlled control whose
visual state only follows the DTO's effective presence, so its rendering
can never desync from the stored overlay either way.

Enum variant switches follow the same shape of rule via the sibling sweep:
a structural `EnsurePresent` whose terminal segment names a declared
variant of the parent enum (a shape-walk check in
`ProjectRegistry::variant_switch_sibling_paths`) clears the overlay entries
at every *sibling* variant path and their subtrees as it is processed —
selecting a variant replaces any pending switch to another variant. When
the switch stores, `SlotOverlay::put_edit`'s parent-scope canonicalization
already does this on server and mirror alike; the load-bearing case is the
switch **back to the base variant**, where the `EnsurePresent` normalizes
away (no stored edit ever reaches `put_edit`) and the registry sweeps the
sibling subtrees explicitly, reporting the cleared entries through
`MutationEffect::Materialized` so ack-mirroring clients follow without a
fetch. Re-selecting the base variant therefore *is* a clean cancel of a
pending switch.

**Known limitation (recorded, no code).** An optional-*wrapped* map or enum
(`Option<Map<…>>`, `Option<Enum<…>>`) would flatten its interior body into
the option row without projecting the interior's gesture facts
(`ui_composite` reads the option's own body), losing the add-entry/variant
affordances. No such shape exists in any demo project today; revisit when
one appears (tracked in the M3 plan notes as future work).

### D8 — Asset bodies ride the same overlay; unapplied editor text is the one client-local exception (added 2026-07-06)

The studio-authoring roadmap's M1 (GLSL asset editing) extends the model to
whole-file asset bodies with **overlay parity**: an applied edit stages one
`MutationOp::SetArtifactBody { artifact, edit: ReplaceBody(bytes) }`,
mirrored as `ArtifactOverlay::Asset` — so D1 (overlay-derived dirty), D4
(the overlay ride-along), and the ack lifecycle of D5 all hold unchanged.
An artifact-keyed pending buffer
(`lpa-studio-core/src/app/project/asset/`) sits beside the slot buffer with
the same phases; per-entry revert is `MutationOp::ClearArtifact`. Asset
edits are persisted-class (no transient bucket); a `.glsl` artifact that
maps to no synced node still counts into the project dirty summary
(`SlotEditJoin::unmapped_asset_dirty_summary`), since it must enable Save.

The editor renders **inline in the asset slot row** (`UiAssetEditor` on
`UiSlotAsset.inline_editor`, resolved per editable asset slot in the node
walk), not as a node-pane tab: the output stays visible beside it, and any
editable asset anywhere in the slot tree gets an editor for free (the shape
the M3 SVG mapping work wants). An earlier node-pane-tab rendering was
replaced (checkpoint tag `checkpoint/asset-editor-tab`).

Four deliberate points:

- **Unapplied editor text is client-local — the one exception to D1.** Text
  typed into the code editor exists only in the editor component until the
  user explicitly applies it (button or Mod-Enter). It is announced by
  editor-local chrome only (a neutral "Modified" chip + Apply enablement),
  never by `DirtySummary`/`UiAffordance` — deliberately outside the
  unsaved-yellow/live-blue color language, because it is neither. The
  editor's clean baseline reconciles against the controller's effective
  content: an ack that catches the external content up to the user's text
  clears the modified state (no imperative "mark clean" call sites).
- **Explicit Apply, no auto-apply.** A mid-keystroke bad compile currently
  stops the node's rendering (no old-shader-keeps-rendering until the
  compiler-robustness budgeted driver lands); revisit auto-apply after
  that.
- **Client-side size guard, no chunking.** `MAX_ASSET_BODY_BYTES` (10 KB)
  parks oversize applies as failed entries client-side; mutations stay
  single-frame under the 16 KB wire budget (`lpc-wire/src/budget.rs`).
  Chunked mutations are recorded future work.
- **Compile errors are presentation-parsed, not wire-structured.** The
  engine's `NodeRuntimeStatus::Error(String)` keeps carrying one rendered
  string; the client best-effort parses the rustc-style
  ` --> <shader>:LINE:COL` marker (`UiShaderError`) into an error strip and
  editor gutter marker. Positions refer to the last applied text and are
  never remapped while the user types — the Modified chip is the honesty
  signal. Structured diagnostics on the wire are recorded future work.

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
