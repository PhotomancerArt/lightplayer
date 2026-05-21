# Phase 04 — Transient read_bytes

**Dispatch:** [sub-agent: yes, model: composer-2.5-fast, parallel: -]

## Scope of phase

Add `ArtifactStore::read_bytes` — read file content through `LpFs` without
retaining bytes on the entry.

**Out of scope:** TOML parsing, registry integration.

## Code Organization Reminders

- Add method to existing `artifact_store.rs`.
- Tests at bottom of file using `lpfs::LpFsMemory`.

## Sub-agent Reminders

- Do **not** commit.
- Returned `Vec<u8>` must not be stored on `ArtifactEntry`.
- Fix warnings properly.

## Implementation Details

### `read_bytes(&mut self, id: &ArtifactId, fs: &dyn LpFs) -> Result<Vec<u8>, ArtifactError>`

1. Look up entry; `UnknownHandle` if missing.
2. Extract `File(path)` from location; internal error if not file.
3. Call `fs.read_file(path)` (or equivalent `LpFs` API).
4. On **Ok(bytes)**: set `read_state = ReadOk`; return bytes.
5. On **Err(e)**: set `read_state = Failed(ArtifactReadFailure::from_fs_error(e))`;
   return matching `ArtifactError::Read(failure)`.

Reading does **not** bump `revision`.

If entry already has `ReadError` from fs delete, read attempt may still run (file
missing) — either outcome is acceptable; prefer updating read_state from actual
read result.

### Tests

Use `LpFsMemory`:

1. **`read_bytes_success_sets_read_ok`** — write file to mem fs, acquire, read,
   assert `ReadOk`, drop returned vec, assert entry still has no payload field.
2. **`read_bytes_missing_file_sets_not_found`** — acquire path not in fs, read fails,
   `Failed(NotFound)` set, entry still exists with refcount.
3. **`read_after_fs_modify_requires_unread_or_reread`** — acquire, read Ok, apply
   Modify fs change (Unread), read again gets new content.

Enable `std` feature on crate for test if mem fs needs it (already default for tests).

Check `LpFs` trait in `lp-base/lpfs/src/lp_fs.rs` for correct read method name.

## Validate

```bash
cargo test -p lpc-node-registry
cargo clippy -p lpc-node-registry --all-targets -- -D warnings
```
