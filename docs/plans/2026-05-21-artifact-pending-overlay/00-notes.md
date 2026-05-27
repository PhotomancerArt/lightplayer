# Artifact pending overlay — planning notes

## Scope of work

Replace the current **materialized** `SlotOverlay` (`DefDraft` / `Bytes` / `Deleted`
snapshots) with an **`ArtifactOverlay`**: a revisioned map of **current pending
changes** keyed by artifact address, projected over committed artifact data.

**In scope (this plan):**

- New overlay storage: `ArtifactLocation` → pending slot/asset edits (path-keyed
  within artifact for slot ops)
- Apply path upserts into the map (no whole-`NodeDef` fork into `DefDraft`)
- Effective read / `NodeDefView` = committed ∪ projected pending
- Commit folds pending map → filesystem → re-sync committed registry state
- Public read API on registry for pending map (client/wire prep)
- Slotted `MapSlot` containers for revision + future wire sync reuse
- Migrate / update existing `lpc-node-registry` tests

**Out of scope (defer):**

- `lpc-wire` message types and client mirror (`future.md`)
- M8 `SessionLog` append-only session layer (superseded for v1 by address-keyed map;
  may revisit for multi-client ordering)
- Engine cutover (`lpc-engine` consuming pending state)
- CRDT / multi-writer merge beyond per-key replace + revision CAS

**Relationship to other docs:**

- Supersedes the **materialized** overlay portion of
  `docs/roadmaps/2026-05-21-changeset-change-management/m8-edit-session-sync/00-design.md`
  (SessionLog → DefDraft fold). This plan uses **map of path → edit** as source of
  truth instead.
- Complements `NodeDefLoc`-only registry identity (recent cutover).
- Wire/sync hardening remains in `docs/roadmaps/2026-05-21-engine-registry-cutover/m1-api-hardening.md`.

## Current state

### What exists today

```
NodeDefRegistry
  store: ArtifactStore
  overlay: SlotOverlay          // BTreeMap<String, SlotOverlayEntry>
  entries: BTreeMap<NodeDefLoc, NodeDefEntry>

SlotOverlayEntry
  Deleted | Bytes(Vec<u8>) | DefDraft(NodeDef)   // materialized snapshots
```

**Apply (slot):** fork committed/overlay → mutate `NodeDef` in memory → store whole
`DefDraft` back. Incoming `SlotEdit` ops are **not retained**.

**Apply (asset):** store raw `Bytes` or `Deleted` tombstone.

**Effective read:** merge overlay snapshot with committed at read time.

**Client pending list:** not available from server; only booleans (`slot_overlay_active`,
`slot_overlay_contains_path`).

### What's wrong for intended UX

- Pending changes are not enumerable or syncable to client
- `DefDraft` duplicates full parsed defs per edit
- Two representations (ops on wire, snapshots in overlay) with no round-trip

### Target model (agreed direction)

```
NodeDefRegistry
  store: ArtifactStore
  overlay: ArtifactOverlay
  defs: BTreeMap<NodeDefLoc, NodeDefEntry>
  root: Option<NodeDefLoc>

ArtifactOverlay
  by_artifact: MapSlot<ArtifactLocation, ArtifactPending>   // Slotted, revisioned

ArtifactPending
  slots: MapSlot<SlotPathKey, SlotEdit>    // one pending edit per slot path (replace)
  asset: Option<AssetPending>              // whole-file pending (replace)

Projection:
  effective_bytes(loc) = apply_pending(store.read(loc), overlay.get(loc.artifact))
  effective_def(loc)   = parse(project) sliced to loc.path for inline defs
```

Not a log — **address → current pending edit**. Repeated edit to same path replaces
the entry.

## Questions — resolved (2026-05-21)

User confirmed plan: **all Q1–Q8 yes**; D1 = string keys v1; D2 = mutual exclusion.

| # | Answer |
|---|--------|
| Q1–Q8 | Yes (per suggested answers) |
| D1 | `MapSlot<String, SlotEdit>` with canonical path string key |
| D2 | Asset pending clears slot map; slot upsert clears asset pending |

## Questions (original)

### Confirmation batch (answer in one pass: `all yes`, or `Q1 yes, Q2 …`)

| # | Question | Context | Suggested answer |
|---|----------|---------|------------------|
| Q1 | Overlay keyed by `ArtifactLocation` (not `String` path)? | Matches `ArtifactStore` + recent identity cutover | Yes |
| Q2 | Within a `.toml` artifact, pending slot edits keyed by `SlotPath` with **replace** semantics? | Map of path → current `SlotEdit`, not append-only | Yes |
| Q3 | At most one **asset** pending per artifact (`Option<AssetEdit>` or enum: None / Delete / ReplaceBody)? | Whole-file replace supersedes prior asset pending | Yes |
| Q4 | **No** `SessionLog` in this plan — overlay map is the pending source of truth? | M8 design had append log → materialized overlay; user rejected log model | Yes |
| Q5 | **Projection on read** for v1 — do **not** add cached `view` field on `NodeDefEntry` yet? | Avoid invalidation complexity; `NodeDefView` computes effective | Yes |
| Q6 | Slotted `MapSlot` from `lpc-model` for overlay containers (not plain `BTreeMap`)? | Reuse revision + future wire snapshot path | Yes |
| Q7 | Plan scope = **`lpc-node-registry` only** (no wire/server changes)? | Wire follows M1/M3 after registry shape is stable | Yes |
| Q8 | Keep `AssetEdit::ReplaceBody` for whole-file TOML escape hatch alongside structured `SlotEdit`? | Already in edit vocabulary; diff uses both | Yes |

### Discussion-style (if confirmation answers differ)

#### D1: SlotPath as MapSlot key

`MapSlot<K,V>` requires `K: MapSlotKeyLike`. `SlotPath` is not implemented today.
Options:

- **A:** `MapSlot<String, SlotEdit>` with canonical path string key (`SlotPath::to_wire()` or existing display)
- **B:** Implement `MapSlotKeyLike for SlotPath` in `lpc-model`

**Suggested:** A for v1 (minimal model churn); B as `future.md` if hot-path allocs hurt.

#### D2: Asset pending vs slot pending on same artifact

A `.toml` file could theoretically have both slot edits and a whole-file
`ReplaceBody`. Options:

- **A:** Mutual exclusion — asset pending clears slot map and vice versa
- **B:** Asset pending wins at projection/commit time
- **C:** Allow both; commit applies asset body then slot ops (or reverse)

**Suggested:** A — asset replace is escape hatch; applying it clears structured slot
pending for that artifact.

## Notes

- User correction: overlay is **not an op log** — it is a **mapping of current pending
  changes** projected over underlying artifact data.
- Slotted system reuse: overlay containers get revision tracking and wire-serializable
  shape; `SlotEdit` / `AssetEdit` remain the edit vocabulary (not meta-mutations on
  overlay slots).
