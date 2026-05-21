# Phase 03 — NodeDefRegistry `load_root` + artifact tracking

**Dispatch:** [sub-agent: yes, model: composer-2.5-fast, parallel: -]

## Scope of phase

Implement `NodeDefRegistry` bootstrap via **`load_root`**, internal entry storage,
artifact acquire/release tracking, and recursive registration via walker.

**In scope:**

- `NodeDefRegistry` fields and **`load_root`** (public)
- Private **`register_file_at_path`** for path-backed invocation recursion
- Parse artifact bytes → entries at root + walked children
- Track which `ArtifactId`s registry holds (for `sync` in phase 04)

**Out of scope:** `sync`, diff/`NodeDefUpdates` emission, `NodeDefView`.

## Code Organization Reminders

- `node_def_registry.rs`: public API top (`load_root`), private helpers bottom,
  tests at end (registration smoke tests only — full gate in phase 05).
- Use phase 01 types and phase 02 walker/shell/resolve helpers.

## Sub-agent Reminders

- Do **not** commit.
- Do **not** edit `lpc-engine`.
- Registry acquires/releases via M1 `ArtifactStore` only.
- **`load_root`** is the only public bootstrap entry point.
- Report deviations.

## Implementation Details

### `NodeDefRegistry` fields

```rust
pub struct NodeDefRegistry {
    entries: BTreeMap<NodeDefId, NodeDefEntry>,
    source_index: BTreeMap<DefSource, NodeDefId>,
    artifact_refs: BTreeMap<ArtifactId, u32>,
    artifact_root_path: BTreeMap<ArtifactId, LpPathBuf>,
    root_id: Option<NodeDefId>,           // id from load_root, if any
    next_id: u32,
}
```

### `load_root` (public)

```rust
pub fn load_root(
    &mut self,
    store: &mut ArtifactStore,
    fs: &dyn LpFs,
    root_path: &LpPath,
    frame: Revision,
    ctx: &ParseCtx<'_>,
) -> Result<NodeDefId, RegistryError>
```

**Steps:**

1. Require absolute `root_path` — return `RegistryError::InvalidPath` if not
   absolute (or normalize if `LpPath` API supports it clearly).
2. `path_buf = root_path.to_path_buf()` (or equivalent).
3. `artifact_id = store.acquire_location(ArtifactLocation::file(path_buf.clone()), frame)`.
4. `registry_acquire_artifact(artifact_id, &path_buf)` — increment
   `artifact_refs`, record `artifact_root_path`.
5. `load_and_register_root(store, fs, artifact_id, &path_buf, frame, ctx)?`.
6. Store `root_id = Some(id)`; return root `NodeDefId`.

Registry must be **empty** on first `load_root` in M2 (return error if not
empty — or document `clear`/reload policy; prefer error on non-empty for M2).

### `register_file_at_path` (private)

Used when walker hits `NodeDefRef::Path(locator)`:

```rust
fn register_file_at_path(
    &mut self,
    store: &mut ArtifactStore,
    fs: &dyn LpFs,
    locator: &ArtifactLocator,
    containing_file: &LpPath,
    frame: Revision,
    ctx: &ParseCtx<'_>,
) -> Result<NodeDefId, RegistryError>
```

1. `resolve_node_locator(containing_file, locator)?` → absolute path.
2. Acquire artifact + register root (same as steps in `load_root` for that file).
3. Return child root `NodeDefId`.

Do **not** expose locator/containing_dir on public API.

### `load_and_register_root` (private)

1. `store.read_bytes(&artifact_id, fs)?` → UTF-8 string.
2. `NodeDef::read_toml(ctx.shapes, &text)` → `NodeDefState`.
3. Insert entry at `DefSource { artifact_id, path: SlotPath::root() }`.
4. If `Loaded`, call `register_invocations(...)` with walker output.

### `register_invocations` (private, recursive)

For each `InvocationSite` from `collect_invocations`:

| `NodeDefRef` | Action |
|--------------|--------|
| `Path(loc)` | `register_file_at_path(store, fs, loc, parent_file, frame, ctx)` |
| `Inline(body)` | Insert entry at `{ parent_artifact_id, site.path }`; recurse into inline body |

**Duplicate `DefSource`:** `RegistryError::DuplicateSource`.

### Entry insertion

```rust
fn insert_entry(
    &mut self,
    source: DefSource,
    state: NodeDefState,
    revision: Revision,
) -> Result<NodeDefId, RegistryError>;
```

### Read failure handling

If `read_bytes` fails or parse fails:

- Still create entry at intended source with error state.
- Do not skip identity creation (roadmap: identity separate from content).

### `release_artifact_if_unused` (private, phase 04)

Decrement `artifact_refs`; at zero call `store.release` and remove path maps.

### Public accessors (for view + tests)

```rust
pub fn root_id(&self) -> Option<NodeDefId>;
pub fn get(&self, id: &NodeDefId) -> Option<&NodeDefEntry>;
pub fn get_by_source(&self, source: &DefSource) -> Option<&NodeDefEntry>;
```

### Smoke tests (this phase)

Use **`load_root`** only (not internal helpers):

1. `load_root("/playlist.toml")` with inline child fixture — root + inline child
   in `source_index`.
2. `load_root("/project.toml")` with path-backed nodes — multiple artifacts in
   `artifact_refs`.
3. Second `load_root` on non-empty registry → error.

## Validate

```bash
cargo test -p lpc-node-registry node_def_registry
cargo clippy -p lpc-node-registry --all-targets -- -D warnings
```
