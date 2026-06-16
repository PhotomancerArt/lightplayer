# Phase 04 — `sync` + diff rules

**Dispatch:** [sub-agent: yes, model: composer-2.5-fast, parallel: -]

## Scope of phase

Implement **`sync`**: detect artifact revision bumps (after driver applies fs
changes to store), re-derive def inventory, emit `NodeDefUpdates` with
shell/body rules and kind-change behavior.

**In scope:**

- **`sync`** on `NodeDefRegistry` (public)
- Per-artifact re-derive and diff against previous inventory
- Shell vs body `changed` classification
- Artifact release for removed defs

**Out of scope:** `NodeDefView`, full gate test suite (phase 05).

## Code Organization Reminders

- Diff logic in `node_def_registry.rs` or `registry/def_diff.rs` if file grows
  large.
- Tests use driver pattern: `apply_fs_changes` then `sync`.
- Tests at bottom of registry module for T1–T4 (T5 in phase 05).

## Sub-agent Reminders

- Do **not** commit.
- Do **not** edit `lpc-engine`.
- Registry must **not** call `store.apply_fs_changes` — tests/driver do that first.
- **Kind change:** report in `changed`; comment documents M6 delete/recreate.
- Report deviations.

## Implementation Details

### `sync` (public)

```rust
pub fn sync(
    &mut self,
    store: &mut ArtifactStore,
    fs: &dyn LpFs,
    frame: Revision,
    ctx: &ParseCtx<'_>,
) -> NodeDefUpdates
```

### Driver sequence (document in module doc)

```rust
store.apply_fs_changes(&changes, frame);
let updates = registry.sync(&mut store, fs, frame, ctx);
```

Registry reads `store.revision(id)` only — does not accept `FsChange` slice in M2.

### Algorithm

1. Collect tracked `ArtifactId`s from `artifact_refs`.
2. For each artifact where `store.revision(id)` differs from last-derived
   revision for that artifact:
   - Build **new inventory** via `derive_artifact_inventory` (shared with phase 03).
3. Build **old inventory** subset for that artifact from current `entries`.
4. Diff:
   - Keys only in old → `removed` (+ schedule artifact ref decrement)
   - Keys only in new → `added`
   - Keys in both → **changed** rules below
5. Apply inventory to `entries` / `source_index`.
6. Release artifacts whose ref count hit zero.
7. Return merged `NodeDefUpdates`.

Call with no pending revision bumps → empty updates.

### Changed rules

| Entry role | Comparison | → `changed` when |
|------------|------------|------------------|
| Root file def / inline leaf | **Body** (`body_changed`) | Full `NodeDef` differs, error state differs, or kind differs |
| Container (`Project`, `Playlist`, …) | **Shell** (`shell_changed`) | Shell differs (inline invocation **kind** stub included) |

**Inline child body edit:** child in `changed` only; parent not in `changed`.

**Entry add/remove:** `added`/`removed` + parent in `changed` when shell differs.

**Kind change:** child in `changed`; parent in `changed` when shell includes that invocation.

**Parse error transitions:** `changed`.

### Shared derive helper

```rust
fn derive_artifact_inventory(
    store: &mut ArtifactStore,
    fs: &dyn LpFs,
    artifact_id: ArtifactId,
    root_path: &LpPath,
    frame: Revision,
    ctx: &ParseCtx<'_>,
) -> Result<BTreeMap<DefSource, DerivedDef>, RegistryError>;
```

Used by `load_root` path and `sync` re-derive.

### Tests (this phase)

All tests: **`load_root`** first, then driver loop.

1. **T1** — `load_root("/clock.toml")`; modify file; `apply_fs_changes`; `sync`
   → root in `changed` only.
2. **T2** — `load_root("/playlist.toml")` inline; edit inline shader slot → child
   `changed`, playlist not.
3. **T3** — add `[entries.3]` → `added` + playlist `changed`.
4. **T4** — path-backed entry; modify child file → child `changed`, playlist not.

Use `LpFsMemory`, `Revision` frames; reference M1 `apply_fs_changes` tests.

## Validate

```bash
cargo test -p lpc-node-registry sync
cargo test -p lpc-node-registry node_def_registry
cargo clippy -p lpc-node-registry --all-targets -- -D warnings
```
