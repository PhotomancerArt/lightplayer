# Phase 02 — Effective Def Parse

**Dispatch:** sub-agent: yes | parallel: -

## Scope of phase

Parse effective TOML for a committed artifact and expose via registry helper.

**In scope:**

- `parse_effective_state(artifact_id, fs, ctx) -> Result<NodeDefState, RegistryError>`
  in `effective_read.rs`
- Uses `artifact_root_path` + `read_effective_bytes`
- UTF-8 + `NodeDef::read_toml` — same as `read_artifact_state` but effective bytes
- Refactor `read_artifact_state` to call `parse_effective_state` **or** keep
  committed path separate (committed = store only; effective = overlay path).
  **Do not** change committed `sync`/`load_root` parse to use overlay.

**Out of scope:** NodeDefView API change (phase 03).

## Key rule

| Path | Bytes source |
|------|----------------|
| `sync` / `load_root` / `entries` | store only (committed) |
| effective parse / view | overlay ∪ store |

## Tests

- Unit: apply SetBytes on `/clock.toml`; `parse_effective_state` shows new field
  values; direct `read_artifact_state` (if still exposed internally) unchanged

## Validate

```bash
cargo test -p lpc-node-registry
```
