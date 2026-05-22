# Commit Contract (v1)

Behavioral contract for `NodeDefRegistry::commit` in M5. Normative for harness
tests; engine cutover is a separate milestone.

## Preconditions

- Registry may have called `load_root` (M5 tests require this).
- Overlay may be empty (no-op commit).
- `fs` must be writable (`LpFs::write_file` / `delete_file`).

## Success path

1. **Resolve** every overlay path to a commit action (bytes write or delete).
2. **Write fs** for all paths before mutating `entries`.
3. **Register** new file paths in `ArtifactStore` (`acquire_file_artifact`).
4. **Bump** store revisions (`apply_fs_changes` or per-path equivalent).
5. **Re-derive** affected defs via `sync_def_artifact` / `sync_source_path`.
6. **Return** `SyncResult` with factual `def_updates`, `source_revisions`, `change_details`.
7. **Clear** overlay.

After success:

- `overlay_active()` is false.
- `registry.get(id)` reflects committed state for affected defs.
- `view().get(id)` equals `registry.get(id)` when overlay is empty.
- Fs files match committed bytes for overlay paths written.

## Failure path

On any error after overlay was non-empty:

- **`entries` / `get()` unchanged** from pre-commit snapshot.
- **Overlay retained** (same pending edits).
- Return `Err(CommitError)`.

Fs may be partially updated on failure; M5 harness should use fixtures where
validation fails before destructive steps, or restore fs in test. Implementation
should serialize/validate before writing when possible.

## Overlay entry mapping

| Entry | Fs | Notes |
|-------|-----|-------|
| `Bytes(b)` | write `b` | assets and TOML escape hatch |
| `SlotDraft(d)` | write `serialize_slot_draft(&d.def, ctx)?` | normal `.toml` authoring |
| `Deleted` | delete path | store marks deleted |

No merge between entry kinds; last apply wins in overlay.

## Precedence (D5)

| Operation | Overlay active on path P | After commit |
|-----------|-------------------------|--------------|
| `view().get` / effective bytes | overlay wins | committed (= fs) |
| `registry.get` | committed only (unchanged pre-commit) | committed updated |
| `sync_fs` on P | bumps store; **does not** clobber overlay reads | normal fs-sync |
| `commit` | promotes overlay → fs + entries | overlay cleared |

## Scope limits (M5)

- Committing new `.toml` files writes fs + store but **does not** add defs to the
  graph unless reachable from existing root via `derive_inventory`.
- Compose-from-blank (A1) is **M6**, not M5.
- Source revision bumps after asset commit follow existing `sync_source_path` rules.

## API surface

```rust
pub fn commit(
    &mut self,
    fs: &dyn LpFs,
    frame: Revision,
    ctx: &ParseCtx<'_>,
) -> Result<SyncResult, CommitError>;
```

`sync()` / `sync_fs()` remain filesystem reload only.
