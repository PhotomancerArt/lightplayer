# Phase 04 — Scenario tests S1–S6

**Dispatch:** [sub-agent: yes, model: composer-2.5-fast, parallel: -]

## Scope of phase

Integration tests calling **`registry.sync`** directly.

**In scope:**

- `tests/fs_change_semantics.rs`
- `harness/fixtures.rs` — load files into `LpFsMemory`, `load_root`, then `sync_fs`
- Assert `SyncResult` fields only (no engine actions)

## Example

```rust
let mut registry = NodeDefRegistry::new();
let fs = fixtures::load_shader_project();
registry.load_root(&fs, LpPath::new("/shader.toml"), frame, &ctx).unwrap();

fixtures::write_file(&mut fs, "/shader.glsl", NEW_GLSL);
let result = registry.sync_fs(&fs, &[fs_change("/shader.glsl")], frame, &ctx);

assert!(result.def_updates.is_empty());
assert!(result.source_revisions.iter().any(|b| b.def_id == shader_id));
```

## Validate

```bash
cargo test -p lpc-node-registry --test fs_change_semantics
```
