# M2 Plan Notes — NodeDefRegistry + NodeDefUpdates

## Scope of work

Implement **`NodeDefRegistry`** in `lpc-node-registry` as the owner of parsed node
definitions, with **`NodeDefUpdates`** reporting def-level deltas when held
artifacts change.

In scope:

- `NodeDefId` opaque handle (same pattern as `ArtifactId`).
- Registry entry source `{ artifact_id, path_in_artifact: SlotPath }` (root =
  `SlotPath::root()`).
- Entry state: `Loaded(NodeDef)` / `ParseError` / `ValidationError` (variant
  reserved; M2 only populates `Loaded` and `ParseError`).
- Registry **acquires file artifacts** via M1 `ArtifactStore` for every
  file-backed def it tracks; **releases** when no defs reference that artifact.
- Inline defs live at non-root paths within a **parent file artifact** — no
  `ArtifactLocation::InlineNode` (rejects old engine pattern).
- **`load_root(root_path)`** bootstrap + **`sync(...)`** steady state →
  `NodeDefUpdates { added, changed, removed }`. Driver owns fs + store;
  applies `apply_fs_changes` before `sync`.
- Parent def **not** marked `changed` when only an inline **child body** edits;
  parent **is** marked `changed` when its shell changes (entry add/remove,
  path↔inline ref flip, non-child slot edits).
- Stub **`NodeDefView`** — reads base registry only (ChangeSet overlay in M5).
- Unit tests: leaf TOML edit, inline child isolation, playlist entry add/remove.

Out of scope:

- `lpc-engine` / `ProjectLoader` edits (**M6**).
- `SourceFileSlot` / GLSL file artifacts (**M3**).
- Engine tree mutation from updates (**M4** harness, **M8** graph).
- Stable `NodeDefId` preservation across ambiguous edits (future).
- Semantic validation beyond TOML parse (**ValidationError** stub only).

## Current state

### M1 (done)

`lpc-node-registry` crate exists with requester-owned `ArtifactStore`:

- `acquire_location` / `acquire_locator` / `release`
- `apply_fs_changes` bumps `revision` on held entries
- Transient `read_bytes(id, fs)` — no cached bytes on entries

`registry/`, `view/`, `source/`, `change/` modules are stubs.

### Engine reference (do not modify in M2)

`lpc-engine` today conflates artifacts + defs:

- `NodeDefHandle { artifact, path: SlotPath }` — non-root paths reserved but
  unused; all defs at artifact root.
- Inline defs use `ArtifactLocation::InlineNode` + synthetic path
  `{project}#nodes.{name}` — **M2 rejects this**; inline defs are registry
  paths only.

### Model parsing

- `NodeDef::read_toml(registry, text) -> Result<NodeDef, NodeDefParseError>`
- `SlotShapeRegistry::default()` sufficient for tests (same as engine).
- `NodeInvocation` appears in:
  - `ProjectDef.nodes` — map key is node name; path = `nodes.{name}`
  - `PlaylistEntry.node` — path = `entries.{key}.node`

### Test fixtures

- `examples/basic/` — leaf file edits (e.g. `clock.toml`).
- `examples/button-playlist/` — path-backed playlist entries today; tests will
  use inline variants (see `playlist_entry.rs` inline test TOML).

Paths must be project-root relative with leading `/` for `LpFsMemory`.

## Architecture sketch (driver model)

```
Driver (tests / engine)
  │
  ├─ load_root("/project.toml")     // any root node .toml; project is convention
  │       └─► registry walks tree, acquires artifacts, registers all defs
  │
  └─ loop:
        store.apply_fs_changes(changes, frame)   // driver-owned
        updates = registry.sync(store, fs, frame, ctx)
        // M5: ChangeSet commit bumps store similarly → same sync
        apply NodeDefUpdates to node tree (M4/M6)
```

Internal walk uses private `register_file_at_path` for path-backed
`NodeInvocation` refs.

### Parent vs child `changed` (shell rule)

Compare two views per def path:

| View | Used for | Inline child body |
|------|----------|---------------------|
| **Shell** | Parent `changed` | Replaced with `Inline { kind }` stub (no nested payload) |
| **Body** | Leaf / inline-child `changed` | Full `NodeDef` equality |

Example: edit inline shader slots under `entries.2.node` → child `changed`,
playlist shell unchanged → parent **not** `changed`.

Example: add `entries.3` → shell changes → parent **changed** + new child
`added`.

## Questions — resolved

### Confirmation batch (Q1–Q7)

| # | Decision |
|---|----------|
| Q1 | Registry **acquires/releases** file artifacts it tracks; inline defs share parent artifact |
| Q2 | Def source = `{ artifact_id, SlotPath }` (`nodes.{name}`, `entries.{key}.node`, root = file def) |
| Q3 | Path-backed children → separate file artifact + root def; child file change → child `changed` only |
| Q4 | `NodeDefUpdates { added, changed, removed: BTreeSet<NodeDefId> }` — no extra inventory field in M2 |
| Q5 | Shell/body split for parent vs inline-child `changed` detection |
| Q6 | `ValidationError` variant exists; only `ParseError` populated in M2 |
| Q7 | Tests use `load_root` on leaf/playlist fixtures; project root supported |

### Q10 — Driver-owned registration (user)

**Decision:**

- **Public API:** `load_root(absolute_path)` once; `sync(...)` after driver
  applies fs changes to `ArtifactStore`.
- Root kind **not** enforced — `project.toml` is convention only.
- Registry does **not** call `apply_fs_changes`; driver owns store + fs.
- M5 ChangeSet commit follows same pattern → `NodeDefUpdates`.
- `register_file_at_path` is **private** (internal path-ref recursion during walk).

### Q8 — ProjectDef registration

**Decision:** Walker supports `Project` + `Playlist`; gate tests focus on leaf
file + playlist inline/path scenarios.

### Q9 — Kind change semantics (user)

**Context:** `NodeKind` is part of node identity for the engine runtime. A def
whose kind changes (e.g. `Shader` → `Clock`) cannot be refreshed in place.

**Decision:**

- Registry still reports the same `NodeDefId` in **`changed`** when kind flips
  (v1 id preservation is best-effort; M6 engine decides recreate either way).
- **Shell stubs** for inline invocations include **`NodeKind`** — a kind flip at
  an invocation site changes the parent shell → parent **`changed`**.
- **Engine contract (M6, document now):** kind change on a bound def → **delete
  and recreate** the runtime node; do not attempt in-place slot refresh.
- M2 tests: include at least one case where inline/path child **kind** changes
  and assert child (and parent shell when applicable) land in `changed`.

## Resolved decisions (roadmap — no re-litigation)

- Parallel build in `lpc-node-registry`; no `lpc-engine` edits until M6.
- ArtifactStore = freshness only; registry = parsed defs.
- No last-good on parse failure — error state on entry (engine cascade in M6).
- Inline child edit must not imply parent `changed` (decisions.md).
- v1 may recreate entries wholesale — stable id preservation deferred.

## Notes from prior discussion

- User agreed to full M2 plan before implementation.
- M1 commit: `bfd0945d` — artifact store complete, 11 tests passing.
