# Artifact Pending Overlay — Design

## Scope of work

Replace materialized `SlotOverlay` (`DefDraft` / `Bytes` / `Deleted` snapshots) with
**`ArtifactOverlay`**: a Slotted, revisioned **map of current pending changes** keyed
by artifact address, **projected** over committed artifact data.

**In:** `lpc-node-registry` overlay storage, apply, projection, commit, introspection,
test migration.

**Out:** `lpc-wire`, `lpa-server`, `SessionLog` (M8), engine cutover.

**Deferred (not v1):** Per-artifact cache of folded effective `NodeDef` to avoid
re-projecting on every path lookup — overlay map stays authoritative; see `future.md`.

## File structure

```
lp-core/lpc-node-registry/src/
├── edit/
│   ├── artifact_overlay.rs          # NEW: ArtifactOverlay, ArtifactPending, AssetPending
│   ├── pending_slot_key.rs          # NEW: SlotPath ↔ stable map key (String)
│   ├── apply.rs                     # UPDATE: upsert into ArtifactOverlay
│   ├── mod.rs                       # UPDATE: exports; remove DefDraft/SlotOverlay
│   ├── slot_edit.rs                 # keep
│   ├── asset_edit.rs                # keep
│   ├── artifact_edit.rs             # keep (ingress vocabulary unchanged)
│   ├── def_draft.rs                 # DELETE
│   └── slot_overlay.rs              # DELETE
├── registry/
│   ├── projection.rs                # NEW: committed + pending → effective
│   ├── node_def_registry.rs         # UPDATE: field rename + pending API
│   ├── slot_apply.rs                # UPDATE: upsert, no DefDraft fork
│   ├── effective_read.rs            # UPDATE: delegate to projection
│   ├── commit.rs                    # UPDATE: fold pending → fs
│   └── node_def_entry.rs            # unchanged v1
├── source/
│   └── materialize.rs               # UPDATE: read asset pending from overlay
├── lib.rs                           # UPDATE: re-exports
└── tests/                           # UPDATE integration tests
```

## Conceptual architecture

```text
  EditBatch / SyncOp::Apply
           │
           ▼
  ┌────────────────────┐
  │ upsert pending     │  SlotEdit  → slots[path] = edit (replace)
  │                    │  AssetEdit → asset = Some(...) (replace; clears slots)
  └─────────┬──────────┘
            │
            ▼
  ┌─────────────────────────────────────────┐
  │ ArtifactOverlay                          │
  │   MapSlot<ArtifactLocation, ArtifactPending> │
  │     ArtifactPending:                     │
  │       slots: MapSlot<String, SlotEdit>   │  // key = canonical SlotPath string
  │       asset: AssetPending                │  // None | Delete | ReplaceBody
  └─────────┬───────────────────────────────┘
            │ project (on read / commit)
            ▼
  ArtifactStore (committed bytes)  ──►  effective bytes / NodeDef
            │
            ▼ commit
       filesystem write
            │
            ▼
  re-sync committed defs; remove overlay keys for committed artifacts
```

### Reads

| API | Returns |
|-----|---------|
| `get(loc)` | Committed `NodeDefEntry` |
| `effective_state(loc)` / `NodeDefView::get(loc)` | Project pending over committed at `loc` |
| `overlay.pending_at(location)` | `Option<&ArtifactPending>` for client sync prep |
| `overlay.is_active()` | Any pending keys exist |

### Pending semantics (not a log)

- **Slot path:** one `SlotEdit` per `SlotPath` key; later edit **replaces** same key.
- **Asset:** at most one `AssetPending` per artifact; setting asset pending **clears**
  slot map for that artifact (mutual exclusion).
- **Delete:** `AssetPending::Delete` tombstone; projection yields missing/deleted bytes.

## Resolved decisions (planning)

| # | Decision |
|---|----------|
| Q1 | Overlay keyed by `ArtifactLocation` |
| Q2 | Slot pending map with replace semantics |
| Q3 | One asset pending per artifact |
| Q4 | No `SessionLog` in this plan |
| Q5 | Projection on read; no cached `NodeDefEntry.view` v1 |
| Q6 | Slotted `MapSlot` containers |
| Q7 | `lpc-node-registry` only |
| Q8 | Keep `AssetEdit::ReplaceBody` escape hatch |
| D1 | String keys for slot paths in `MapSlot` (v1) |
| D2 | Asset pending and slot pending mutually exclusive per artifact |

## Validation (full plan)

```bash
cargo test -p lpc-node-registry
cargo check -p lpc-node-registry --no-default-features
just check   # final phase only (if lints touched)
```
