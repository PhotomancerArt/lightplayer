# Phase 01 — Effective Bytes Read

**Dispatch:** sub-agent: yes | parallel: -

## Scope of phase

Implement `NodeDefRegistry::read_effective_bytes` — overlay before store.

**In scope:**

- New `registry/effective_read.rs` with byte resolution logic
- `read_effective_bytes(&mut self, path: &LpPath, fs) -> Result<Option<Vec<u8>>, RegistryError>`
- Handle overlay `Bytes`, `Deleted` (return `Ok(None)`)
- Fall through to `artifact_path_to_id` + `store.read_bytes` when no overlay
- Path not in store and not in overlay → `Ok(None)`
- Unit tests on registry or `effective_read` module

**Out of scope:** NodeDefView, TOML parse, materialize.

## Implementation details

**Overlay precedence:**

1. If `overlay.contains_path(path)`:
   - `Deleted` → `Ok(None)`
   - `Bytes(b)` → `Ok(Some(b))`
2. Else if `artifact_path_to_id` has path → `store.read_bytes`
3. Else → `Ok(None)`

**Requires `&mut self`** because `store.read_bytes` mutates read state (matches
existing `read_artifact_state` pattern).

**Tests:**

- overlay SetBytes wins over store for loaded shader.glsl
- no overlay delegates to store
- overlay Delete returns None
- implicit-create overlay path returns bytes without store entry

## Validate

```bash
cargo test -p lpc-node-registry effective
cargo check -p lpc-node-registry
```
