# UI Parity — WireSlotMutation vs Edit Language

**Status:** draft for M1 review  
**Goal:** Define v1 cutover blockers vs post-cutover enhancements.

## Current debug UI flow

```text
User edits control in lp-cli debug UI
  → SlotEditIntent { root: "node.<id>.def", path, value }
  → SlotMirrorView.prepare_set_value (client-side validation + revision CAS)
  → WireSlotMutationRequest piggybacked on ProjectReadRequest
  → lpa-server → engine.mutate_project_slots (immediate in-memory write)
  → WireSlotMutationResponse accept/reject
  → SlotMirrorView.apply_mutation_response
```

No commit step. No overlay. Effective = committed = engine memory.

## Target flow (after cutover)

```text
User edits control
  → Resolve (artifact_path, slot_path)   ← NEW: needs read metadata (M1 C2)
  → Build ArtifactEdit::Slot { AssignValue, … }
  → SyncOp::Apply (pending overlay)
  → Optional: read effective via project read
  → User commits → SyncOp::Commit → fs + engine refresh
```

## Capability matrix

| # | User action | Today | Edit language | v1 blocker? | Notes |
|---|-------------|-------|---------------|-------------|-------|
| 1 | Edit scalar on node def (clock rate, shader param) | `SetValue` | `AssignValue` | **Yes** | Core debug UI |
| 2 | See edit errors inline | `WireSlotMutationRejection` | TBD wire rejection | **Yes** | Map error enums in M1 |
| 3 | Pending / in-flight indicator | per-slot mutation id | client pending overlay | **Yes** | Redesign mirror model |
| 4 | Optimistic local preview | pending queue, no local write | overlay mirror or wait for read | **TBD** | M1 B4 |
| 5 | Change node kind | not in UI | `UseEnumVariant` | No v1? | Unless needed for examples |
| 6 | Map / playlist entry edits | not in UI | `MapInsert`, etc. | No v1 | Harness only |
| 7 | Option fields | not in UI | `UseOption` | No v1 | |
| 8 | Edit GLSL source | not via mutation | `AssetEdit::ReplaceBody` | No v1 | Future asset editor |
| 9 | Commit / discard | N/A (instant) | explicit ops | **Yes** | UX change — add UI affordance? |
| 10 | Node tree slot read | `node.<id>.def` roots in project read | effective defs from registry | **Yes** | Read path must use NodeDefView |

## Addressing migration

Today `SlotEditKey` = `{ root: "node.3.def", path }`.

Target key = `{ artifact_path: "/clock.toml", path }` or keep logical root if read
continues to expose `node.<id>.def` snapshots but **edits** use paths.

**M1 decision needed:** dual keys during transition vs one-time UI rewrite.

## Read metadata needed (if path-centric edits)

Project read should expose per node (minimum):

- `node_id`
- `def_artifact_path` (absolute)
- optional `slot_path_prefix` for inline children (e.g. `entries[2].node.def`)

Without this, UI cannot build `ArtifactEdit` from the node panel.

## Suggested v1 parity bar

**Blockers:** rows 1, 2, 3, 9, 10  
**Defer:** rows 5–8  
**Decide in M1:** row 4 (optimistic preview strategy)
