# M0.1 — Pre-M1 stabilization

Small refactors and design notes before M1 API hardening sign-off.

### Store-owned artifact catalog (M0.1b)

`ArtifactStore` now owns durable path ↔ [`ArtifactId`] registration:

- `register_file` / `unregister` — catalog lifetime (not refcount cache pins)
- `id_for_path` / `path_for_id` — lookups
- Registry duplicate maps removed; `reconcile_artifacts` unregisters ids not
  referenced by defs or source deps

Ids remain stable for the same path until explicit unregister.

### `ArtifactStore` keys use `ArtifactId`

`by_handle: BTreeMap<u32, _>` and `location_to_handle: BTreeMap<_, u32>` were
wrong — maps now use `ArtifactId` directly:

- `by_id: BTreeMap<ArtifactId, ArtifactEntry>`
- `location_to_id: BTreeMap<ArtifactLocation, ArtifactId>`

Allocation counter stays `next_id: u32`; `alloc_id()` returns `ArtifactId`.

### Terminology: id, not handle

- `ArtifactId::handle()` → `ArtifactId::raw()` (inner u32 for logging/errors)
- `ArtifactError::UnknownHandle` → `UnknownArtifact { id: ArtifactId }`
- `ArtifactError::InvalidRelease { handle }` → `{ id: ArtifactId }`
- `EditError::UnknownArtifact { artifact_id: u32 }` → `{ id: ArtifactId }`
- `ArtifactId` serde is `#[serde(transparent)]` (wire: plain number)

### Serde: PascalCase `kind` tags

For internally tagged enums where `kind` names a **type**, variant names stay
PascalCase on the wire:

```json
{ "kind": "Slot", "target": { ... }, "ops": [ ... ] }
{ "kind": "Asset", "target": { ... }, "ops": [ ... ] }
```

No `rename_all = "snake_case"` on the enum when `tag = "kind"`. Field names and
nested op variant names may still use snake_case where they are not type tags.

---

## Open for M1: edit batch shape

### Problem: `EditTarget` mixes wire and storage concerns

Today each `ArtifactEdit` carries `target: EditTarget`:

```rust
EditTarget::Id(ArtifactId)   // registry-internal
EditTarget::Path(LpPathBuf)  // authoring / implicit overlay create
```

That is convenient for a single apply path, but **stored pending edits on the
server should always be keyed by `ArtifactId`**. Path is a client authoring
concern: resolve once at apply ingress, then never persist path in the batch.

**Proposal:**

| Layer | Target form |
|-------|-------------|
| Wire / client apply | Path only (M1 default A3) |
| Registry overlay + pending batch | `ArtifactId` only |
| `EditTarget` | Keep for wire ingress helper, or split into `WireArtifactTarget` vs drop from stored types |

Apply flow: `Path` → register/acquire artifact if needed → merge into pending
map by id.

### Problem: `EditBatch` is `Vec<ArtifactEdit>`

A vec allows multiple blocks for the same artifact (duplicate targets, ambiguous
merge order). That does not match mental model: **one pending body per artifact
per batch**.

**Proposal — stored form:**

```rust
pub struct EditBatch {
    pub id: EditBatchId,
    pub artifacts: BTreeMap<ArtifactId, ArtifactBodyEdit>,
}

pub enum ArtifactBodyEdit {
    Slot(Vec<SlotEdit>),
    Asset(Vec<AssetEdit>),
}
```

Properties:

- At most one entry per `ArtifactId` (map invariant)
- Slot vs asset is explicit on the value — no repeated `target` per block
- Ops within one artifact still ordered (`Vec<SlotEdit>` / `Vec<AssetEdit>`)

**Wire form** (optional separate type or same with path key before resolution):

```rust
// ingress only — converted before persistence
pub struct WireEditBatch {
    pub id: EditBatchId,
    pub edits: Vec<WireArtifactEdit>,
}

pub enum WireArtifactEdit {
    Slot { path: LpPathBuf, ops: Vec<SlotEdit> },
    Asset { path: LpPathBuf, ops: Vec<AssetEdit> },
}
```

Server: resolve each path → id, merge ops into `BTreeMap`, reject duplicate
path/id in one batch.

### Overlay alignment

`SlotOverlay` is still path-keyed today. For server storage consistency,
consider:

- Overlay keyed by `ArtifactId` with path lookup via `artifact_path_to_id`, or
- Keep path keys in overlay but ensure batch/pending metadata is id-keyed

Decision deferred to M1 — note dependency on implicit-create semantics (new
path before first commit).

### Migration from current `ArtifactEdit`

Current shape can be mechanically translated:

```text
Vec<ArtifactEdit>  →  fold into BTreeMap<ArtifactId, ArtifactBodyEdit>
  Slot { target, ops }  →  resolve(target) → ArtifactBodyEdit::Slot(ops)
  Asset { target, ops } →  resolve(target) → ArtifactBodyEdit::Asset(ops)
```

Reject batch if two blocks resolve to the same id with conflicting kinds
(Slot vs Asset on same file is an error).

---

## M1 questions to add

| # | Question |
|---|----------|
| E1 | Stored `EditBatch`: `BTreeMap<ArtifactId, ArtifactBodyEdit>` vs vec + validation? |
| E2 | Separate wire ingress type vs single type with optional path/id? |
| E3 | Overlay key: path vs `ArtifactId`? |
| E4 | Duplicate artifact in one batch: error vs last-wins merge? |

Suggested defaults: **map + error on duplicate**, **wire path-only ingress**,
**overlay id-keyed when server owns registry** (path ok for harness-only until M3).
