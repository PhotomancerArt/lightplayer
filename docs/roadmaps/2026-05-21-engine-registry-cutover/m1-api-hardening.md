# Milestone 1: API Hardening + Cutover Readiness

## Title and goal

**Do not move code yet.** Decide and document the committed API shape for edit
vocabulary, wire messages, client parity, and cutover sequencing — then implement
only what M1 needs to prove the shape (small spikes optional).

M1 is the gate: we should be confident in the types and boundaries before M2+
implementation churn.

## Suggested plan location

`docs/roadmaps/2026-05-21-engine-registry-cutover/m1-api-hardening/`

Deliverables live in that plan directory:

- `00-notes.md` — question log + answers
- `00-design.md` — frozen API (after review)
- `ui-parity.md` — debug UI / client capability matrix
- `mutation-inventory.md` — legacy mutation stack to delete post-cutover
- `m3-m4-sequencing.md` — server-first vs combined cutover options

## Scope

**In:**

- All open design questions (below) resolved or explicitly deferred with owner
- UI parity analysis vs current `WireSlotMutation` + debug UI
- Legacy mutation inventory + cleanup checklist (execution in M8; list in M1)
- Target module layout (`lpc-model::edit`, `lpc-wire`, registry boundaries)
- Optional: type move spike **only if** design is signed off mid-M1

**Out:**

- Full wire implementation (M2)
- Server or engine cutover (M3+)
- Graph reconciliation

## Question catalog (resolve in M1)

### A. Model vs registry split

| # | Question | Context | Suggested default |
|---|----------|---------|-------------------|
| A1 | Which types live in `lpc-model::edit`? | Portable edit vocabulary must be shared by wire and registry | `ProjectOverlay`, `ArtifactOverlay`, `SlotOverlay`, `SlotEdit`, `SlotEditOp`, `ArtifactBodyEdit`, `OverlayMutation`, mutation ids/results, portable definition locations |
| A2 | Does `SyncOp` live in model? | `SyncOp` mixes client edits with server-local fs events | **No** — client wire uses overlay mutations; registry `SyncOp::Fs` remains local |
| A3 | `EditTarget::Id(ArtifactId)` on wire? | Id is registry-internal | **Path only** on wire; registry resolves id locally if kept in model |
| A4 | `EditError` in model vs wire-only rejections? | Two layers today | Model: apply errors; wire: request rejections + mapping table |
| A5 | `schemars` on model edit types? | Wire has schema-gen | Mirror `lpc-wire` pattern via model feature flag |
| A6 | `EditBatchId` semantics | Unused in sync today | Define: client correlation id vs server idempotency (or neither v1) |

### B. Wire envelope

| # | Question | Context | Suggested default |
|---|----------|---------|-------------------|
| B1 | Piggyback on `ProjectReadRequest` vs new message? | Mutations already piggybacked | TBD — document tradeoffs in M1 |
| B2 | Wire op set = full `SyncOp` or subset? | Fs events are server-local | Client: read overlay, mutate overlay, commit overlay; no Fs on wire |
| B3 | Response carries `SyncOutcome`? | Today only mutation accept/reject | Extend `ProjectReadResponse` with pending + commit summary |
| B4 | Optimistic concurrency model? | Slot mutation uses shape/data `Revision` CAS | Overlay model: pending until commit; define conflict rules for concurrent Apply |

### C. Addressing

| # | Question | Context | Suggested default |
|---|----------|---------|-------------------|
| C1 | Artifact path vs `node.<id>.def`? | Two vocabularies today | **Path + SlotPath** for edits; see UI parity |
| C2 | How does UI resolve node → artifact path? | Debug UI uses string roots | Project read metadata: `NodeId` → `(artifact_path, path_prefix)` |
| C3 | Inline playlist children | Edits use paths on parent `.toml` | Document prefix rules; no separate wire root |

### D. M3 / M4 sequencing (open — user TBD)

| # | Question | Context | Options |
|---|----------|---------|---------|
| D1 | Server registry without engine update? | M3 applies/commits to fs; engine stale until M4 | **A:** M3 server-only staging · **B:** merge M3+M4 · **C:** M4 first on host harness |
| D2 | When does UI see effective edits? | Overlay ≠ engine tree until cutover | Document UX per option; may accept lag if cutover is fast |
| D3 | Single project open on server? | Registry per project | Confirm lifecycle: load root, unload on close |

### E. Commit / pending UX

| # | Question | Context | Suggested default |
|---|----------|---------|-------------------|
| E1 | Explicit Commit on wire? | Registry commit is exposed through `WireOverlayCommit*` | **Yes** — client drives commit; server does not auto-commit edits |
| E2 | Discard / ClearPending exposure? | Registry supports both | Wire both for editor reset |
| E3 | Read effective vs committed in project read? | `NodeDefView` vs `get()` | Define query flag or always effective for editor |

## UI parity (document in `ui-parity.md`)

Current production edit path (debug UI):

| Capability | Today (`WireSlotMutation`) | Edit language equivalent | M1 note |
|------------|---------------------------|--------------------------|---------|
| Value leaf edit | `SetValue` on `node.<id>.def` + `SlotPath` | `AssignValue` on artifact path + path | Needs C2 mapping |
| Enum / kind change | Not on wire | `EnsurePresent` on variant path | POC |
| Map insert/remove | Not on wire | `EnsurePresent` / `Remove` | POC |
| Option some/none | Not on wire | `EnsurePresent` / `Remove` | POC |
| Artifact body replace | Not on wire | `ArtifactBodyEdit::ReplaceBody` | POC |
| Artifact delete | Not on wire | `ArtifactBodyEdit::Delete` | POC |
| Pending indicator | `SlotMirrorView.pending` + mutation id | Client overlay mirror TBD | Redesign in M2/M3 |
| Conflict handling | shape/data revision CAS | TBD (B4) | M1 must decide |
| Commit | Immediate apply to engine memory | `WireOverlayCommitRequest` | **Behavior change** — UI must add commit |
| Error display | `WireSlotMutationRejection` | `EditError` / wire rejection | Map in M1 |

**M1 deliverable:** explicit v1 parity target — which rows are cutover blockers vs
post-cutover enhancements.

## Legacy mutation cleanup inventory (document in `mutation-inventory.md`)

Remove after cutover (execution **M8**; inventory **M1**):

| Area | Symbols / files |
|------|-----------------|
| `lpc-wire` | `WireSlotMutationRequest`, `WireSlotMutationOp`, responses, rejections |
| `lpc-view` | `SlotMirrorView::prepare_set_value`, `PendingSlotMutation`, pending queue |
| `lpc-engine` | `slot_mutation.rs`, `mutate_project_slots` |
| `lpa-server` | `apply_project_mutations`, mutation logging |
| `lp-cli` | `SlotEditIntent`, `prepare_queued_mutations`, mutation status UI |
| `lpc-shared` | server trait mutation hook |

Also: engine in-memory def mutation path vs overlay+commit.

## M1 exit criteria (gate for M2)

- [ ] `00-design.md` reviewed — frozen type list and crate boundaries
- [ ] All A–E questions answered or deferred with milestone tag
- [ ] `ui-parity.md` — v1 blocker list agreed
- [ ] `mutation-inventory.md` — complete
- [ ] `m3-m4-sequencing.md` — chosen option (A/B/C)
- [ ] ChangeSet `change-language.md` + decisions updated to point at model home
- [ ] Optional: types moved to model if design closed early

## Dependencies

- ChangeSet M10 slot/asset split landed on branch

## Execution strategy

**Full plan** — design review milestone; implementation is mostly docs + small
spikes. Type move can be last phase of M1 or first task of M2 depending on review.
