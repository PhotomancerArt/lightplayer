# Phase 03 — ArtifactStore acquire, release, fs changes

**Dispatch:** [sub-agent: yes, model: composer-2.5-fast, parallel: -]

## Scope of phase

Implement `ArtifactStore` with:

- `new`, `acquire_location`, `acquire_specifier`, `release`
- `apply_fs_changes`
- `revision`, `entry`, `refcount` accessors

**Out of scope:** `read_bytes` (phase 04). No `lpc-engine` edits.

## Code Organization Reminders

- Store impl in `artifact_store.rs`; entry points at top, private helpers at bottom.
- Tests at bottom of `artifact_store.rs`.
- Field name **`revision`**, not `content_frame`.

## Sub-agent Reminders

- Do **not** commit.
- Requester-owned model: entries only from acquire; fs never creates entries.
- Report deviations.

## Implementation Details

### `ArtifactStore` fields

```rust
pub struct ArtifactStore {
    by_handle: BTreeMap<u32, ArtifactEntry>,
    location_to_handle: BTreeMap<ArtifactLocation, u32>,
    next_handle: u32,
}
```

### `acquire_location(location, frame: Revision) -> ArtifactId`

- If location exists: increment `refcount`, return same id.
- Else: allocate handle, insert entry with:
  - `refcount = 1`
  - `revision = frame`
  - `read_state = Unread`

### `acquire_specifier(locator, frame) -> Result<ArtifactId, ArtifactError>`

Resolve via `ArtifactLocation::try_from_specifier`, then `acquire_location`.

### `release(id, _frame) -> Result<(), ArtifactError>`

- Decrement `refcount`; error if unknown handle or already zero.
- At **refcount 0**: remove entry from `by_handle` and `location_to_handle`.

### `apply_fs_changes(changes: &[FsChange], frame: Revision)`

For each change, find entry by **path match** on `ArtifactLocation::File(path)`:

- **No matching acquired entry** → skip (fs does not register artifacts).
- **`Modify` / `Create`**: set `revision = frame`, `read_state = Unread`.
- **`Delete`**: set `revision = frame`, `read_state = Failed(Deleted)`.

Path match: compare `FsChange.path` to entry's file path (`LpPathBuf` equality).

### Tests (required)

1. `acquire_same_location_reuses_handle_and_increments_refcount`
2. `release_at_zero_removes_entry`
3. `fs_modify_bumps_revision_and_sets_unread` — acquire, apply Modify, assert revision + Unread
4. `fs_change_on_unacquired_path_is_noop` — apply change before acquire, then acquire gets fresh revision from acquire frame only
5. `fs_delete_sets_deleted_failure_while_entry_held`
6. `acquire_specifier_rejects_lib`

Use `lpfs::FsChange`, `ChangeType`, `LpPathBuf`.

## Validate

```bash
cargo test -p lpc-node-registry
cargo clippy -p lpc-node-registry --all-targets -- -D warnings
```
