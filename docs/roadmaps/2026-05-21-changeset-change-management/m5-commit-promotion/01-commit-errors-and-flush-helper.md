# Phase 01 — Commit Errors + Overlay Flush Helpers

**Dispatch:** sub-agent: main | parallel: -

## Scope of phase

Add `CommitError`, overlay iteration/resolution helpers, and unit-level flush
logic without wiring public `commit()` yet.

**In scope:**

- `change/commit_error.rs` — `CommitError` variants (empty overlay ok, serialize,
  fs write, re-derive)
- Export from `change/mod.rs` and `lib.rs`
- `registry/commit.rs` (included from `node_def_registry.rs` via `#[path]`) with:
  - `resolve_overlay_bytes(path, entry, ctx) -> Result<Option<Vec<u8>>, CommitError>`
  - `overlay_paths(overlay) -> Vec<LpPathBuf>` iterator
  - `is_def_artifact_path(path) -> bool` (`.toml` suffix)
- Tests for serialize resolution (SlotDraft → bytes) in commit module unit tests

**Out of scope:** public `commit()`, integration tests, fs writes.

## Implementation details

- Reuse `serialize_slot_draft` from `slot_apply.rs`.
- `Deleted` → `Ok(None)` from byte resolver; caller maps to fs delete.
- `CommitError` should convert from `ChangeError` / `RegistryError` where helpful.

## Sub-agent reminders

- Do not commit.
- Do not expand scope into engine or lpc-model beyond existing serialize path.

## Validate

```bash
cargo test -p lpc-node-registry commit
cargo check -p lpc-node-registry
```
