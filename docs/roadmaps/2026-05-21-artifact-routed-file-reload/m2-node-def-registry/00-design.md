# M2 Design — NodeDefRegistry + NodeDefUpdates

## Scope

Implement **`NodeDefRegistry`** in `lpc-node-registry`: parsed `NodeDef`
storage keyed by `NodeDefId`, driven by M1 `ArtifactStore` freshness, reporting
**`NodeDefUpdates`** on artifact changes. Stub **`NodeDefView`** for future
ChangeSet overlay (M5).

No `lpc-engine` edits.

## Ownership model

```
┌─────────────┐
│   Driver    │  tests / engine (M6) / M4 harness
│  owns fs +  │
│ ArtifactStore│
└──────┬──────┘
       │
       │ bootstrap (once): load_root("/project.toml")  // any root .toml kind
       │ loop: store.apply_fs_changes(...); registry.sync(...) → NodeDefUpdates
       ▼
┌──────────────────┐     acquire/release     ┌────────────────┐
│ NodeDefRegistry  │ ◄──────────────────────►│ ArtifactStore  │
└──────────────────┘                         └────────────────┘
       │
       │ M5: ChangeSet commit → same NodeDefUpdates shape
       ▼
 NodeDefUpdates { added, changed, removed }
```

- **Driver** owns `LpFs` and **`ArtifactStore`**. Registry does **not** call
  `apply_fs_changes` — driver applies fs (or future ChangeSet commit) to the
  store first, then calls **`sync`**.
- Registry is the **requester** for file artifacts backing defs it tracks.
- **Inline defs** live at non-root `SlotPath` within a **parent file artifact**
  — no `ArtifactLocation::InlineNode`.
- When the last registry entry referencing a file artifact is removed, registry
  **`release`s** that artifact.

## Driver API (public contract)

Two entry points for M2. Everything else is private.

### Bootstrap — `load_root`

```rust
pub fn load_root(
    &mut self,
    store: &mut ArtifactStore,
    fs: &dyn LpFs,
    root_path: &LpPath,   // absolute, e.g. "/project.toml"
    frame: Revision,
    ctx: &ParseCtx<'_>,
) -> Result<NodeDefId, RegistryError>
```

- Called **once** per load (or full reload).
- **`root_path`** points at any node-definition TOML. Convention is
  `project.toml`, but kind is **not** enforced — `load_root("/playlist.toml")`
  is valid.
- Acquires the file artifact, parses root def, walks invocations recursively,
  registers all discovered defs.
- Returns the **`NodeDefId`** for the root entry (`SlotPath::root()` on that
  artifact).

### Steady state — `sync`

```rust
pub fn sync(
    &mut self,
    store: &mut ArtifactStore,
    fs: &dyn LpFs,
    frame: Revision,
    ctx: &ParseCtx<'_>,
) -> NodeDefUpdates
```

- Called after driver has applied **`store.apply_fs_changes(changes, frame)`**
  (M2) or equivalent bumps from ChangeSet commit (M5).
- Re-derives defs for artifacts whose **`revision`** advanced; returns
  **`NodeDefUpdates`**.
- Safe to call with no pending changes — returns empty sets.

### Private helpers (not public)

- `register_file_at_path` — recursive registration for path-backed
  `NodeInvocation` refs (internal to walk).
- `derive_artifact_inventory`, `register_invocations`, etc.


## File structure

```
lp-core/lpc-node-registry/src/
├── lib.rs                          # re-export registry + view types
├── registry/
│   ├── mod.rs
│   ├── node_def_id.rs              # opaque NodeDefId
│   ├── def_source.rs               # { artifact_id, path: SlotPath }
│   ├── node_def_state.rs           # Loaded | ParseError | ValidationError
│   ├── node_def_entry.rs           # source + state + last_seen_revision
│   ├── node_def_updates.rs         # { added, changed, removed }
│   ├── registry_error.rs
│   ├── parse_ctx.rs                # ParseCtx { shapes: &SlotShapeRegistry }
│   ├── def_shell.rs                # shell equality (inline → kind stub)
│   ├── def_walker.rs               # walk Project + Playlist invocations
│   └── node_def_registry.rs        # load_root, sync (+ private walk/register)
└── view/
    ├── mod.rs
    └── node_def_view.rs            # stub: base registry lookup
```

## Core types

### DefSource

```rust
pub struct DefSource {
    pub artifact_id: ArtifactId,
    pub path: SlotPath,  // SlotPath::root() = artifact root file def
}
```

Maps 1:1 to future engine `NodeDefHandle` (M6 rename).

### NodeDefState

```rust
pub enum NodeDefState {
    Loaded(NodeDef),
    ParseError(NodeDefParseError),
    ValidationError(/* reserved — unused in M2 */),
}
```

- Entry **always exists** when registry knows a def should exist at a source
  (identity separate from content — roadmap error semantics).
- Artifact read failure during derive → entry enters `ParseError` (or dedicated
  read error variant wrapped for display); no last-good retention.

### NodeDefUpdates

```rust
pub struct NodeDefUpdates {
    pub added: BTreeSet<NodeDefId>,
    pub changed: BTreeSet<NodeDefId>,
    pub removed: BTreeSet<NodeDefId>,
}
```

Def-level deltas only. Engine tree mutation is out of scope (M4/M8).

### ParseCtx

```rust
pub struct ParseCtx<'a> {
    pub shapes: &'a SlotShapeRegistry,
}
```

Tests use `SlotShapeRegistry::default()` (same as engine).

## Derive pipeline

### load_root (public)

1. Require **absolute** `root_path` (driver responsibility).
2. `store.acquire_location(ArtifactLocation::file(root_path), frame)`.
3. `read_bytes` → `NodeDef::read_toml(ctx.shapes, text)`.
4. Register root entry at `{ artifact_id, SlotPath::root() }`.
5. **`def_walker`**: discover nested defs (see below).
6. Record `last_seen_revision = store.revision(artifact_id)`.

Returns root `NodeDefId`. Store **`root_path`** on registry (optional field) for
driver introspection; walker uses per-artifact paths from `artifact_root_path`
map.

### def_walker (private, invoked from load_root / sync derive)

Invocation sites in v1 model:

| Parent `NodeDef` | Field | Child path suffix |
|------------------|-------|-------------------|
| `Project` | `nodes.{name}` | `nodes.{name}` (value is `NodeInvocation`) |
| `Playlist` | `entries.{key}.node` | `entries.{key}.node` |

For each `NodeInvocation`:

| `NodeDefRef` | Action |
|--------------|--------|
| `Path(locator)` | Resolve relative to **containing artifact file path** → recursive **`register_file_at_path`** → child at **child artifact root** |
| `Inline(def)` | Register entry at `{ parent_artifact_id, path }` with full body; recurse walker into inline `NodeDef` body for nested invocations |

### sync (public)

For each **tracked file artifact** where `store.revision(id)` differs from
last-derived revision for that artifact:

1. Re-read bytes and re-derive full def inventory for that artifact subtree.
2. Diff old vs new `DefSource → entry` maps:
   - In old, not new → **`removed`**
   - In new, not old → **`added`**
   - In both → compare for **`changed`** (rules below)
3. Update `last_seen_revision` on success.
4. Release artifacts no longer referenced.

**Driver sequence (M2 tests and M6 engine):**

```rust
store.apply_fs_changes(&changes, frame);
let updates = registry.sync(&mut store, fs, frame, ctx);
// apply updates to node tree (engine M6 / harness M4)
```

## Changed detection: shell vs body

### Body equality (leaf + inline child entries)

Full `NodeDef` equality at the entry's source path. Used for defs that **are**
the invoked definition (artifact root file defs, inline child defs).

### Shell equality (parent entries that contain invocations)

For parent defs (`Project`, `Playlist`, or any future container), compute a
**shell** view:

- Walk the `NodeDef`; at each `NodeInvocation` with `NodeDefRef::Inline(_)`,
  replace the inline body with a **kind-only stub** (`NodeDef` of same
  `NodeKind`, default/minimal payload).
- Path-backed invocations compare by **resolved locator string** (and presence).

Parent is **`changed`** when shell differs. Inline **child body** edit without
shell change → child **`changed`**, parent **not** **`changed`**.

### Kind change (engine contract)

**Any `NodeKind` change on a bound def requires runtime delete/recreate** (M6).
Registry reports the existing `NodeDefId` in **`changed`**; engine must not
treat kind change as in-place slot refresh.

Kind change effects on updates:

| Scenario | Registry report |
|----------|-------------------|
| Inline child kind flip (e.g. `Shader` → `Clock`) | Child **`changed`**; parent shell includes invocation kind → parent **`changed`** |
| Root file def kind flip | Root **`changed`** |
| Path-backed child file kind flip | Child root **`changed`**; parent shell unchanged if locator string unchanged |

M2 tests must cover at least one kind-change scenario.

## NodeDefView (stub)

```rust
pub struct NodeDefView<'a> {
    registry: &'a NodeDefRegistry,
}

impl<'a> NodeDefView<'a> {
    pub fn get(&self, id: &NodeDefId) -> Option<&NodeDefEntry>;
}
```

No ChangeSet overlay (M5). No mutation API.

## Test scenarios (gate)

| # | Scenario | Setup | Expected updates |
|---|----------|-------|------------------|
| T1 | Leaf file edit | `load_root("/clock.toml")`; modify file; `apply_fs_changes` + `sync` | Root in `changed` |
| T2 | Inline child edit | `load_root("/playlist.toml")` with inline entry; edit inline shader slot | Child `changed`; playlist **not** `changed` |
| T3 | Entry add/remove | `load_root("/playlist.toml")`; add/remove `[entries.N]` | New/removed child ids; playlist in `changed` |
| T4 | Path-backed child file edit | `load_root("/playlist.toml")` with path entry; modify child file | Child `changed`; playlist **not** `changed` |
| T5 | Inline child kind change | `load_root("/playlist.toml")`; flip inline kind | Child + playlist in `changed` |

Use `LpFsMemory` + inline TOML fixtures (adapt patterns from
`playlist_entry.rs` tests). Optionally load paths under `examples/basic/`.

## Validation

```bash
cargo +nightly fmt --all
cargo test -p lpc-node-registry
cargo clippy -p lpc-node-registry --all-targets -- -D warnings
```

## Out of scope

- `lpc-engine` cutover (M6).
- `SourceFileSlot` / asset artifacts (M3).
- ChangeSet overlay (M5).
- Stable `NodeDefId` across ambiguous edits.
- Semantic `ValidationError` population.

## Plan phases (dispatch)

| # | Phase | Dispatch |
|---|-------|----------|
| 01 | Registry types + ParseCtx | [sub-agent: yes, model: **composer-2.5-fast**] |
| 02 | Def walker + shell helpers | [sub-agent: yes, model: **composer-2.5-fast**] |
| 03 | NodeDefRegistry `load_root` + artifact tracking | [sub-agent: yes, model: **composer-2.5-fast**] |
| 04 | `sync` + diff rules | [sub-agent: yes, model: **composer-2.5-fast**] |
| 05 | NodeDefView stub + gate tests + cleanup | [sub-agent: **supervised**, model: **composer-2.5-fast**] |

Phases run sequentially. Single commit at end of plan per `/implement`.
