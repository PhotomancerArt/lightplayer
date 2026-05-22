# M6 Summary — Diff + Equivalence Gate

## Status

Implemented on branch `codex/incremental-artifact-reload`.

## Delivered

### lpc-node-registry

- `diff/` module — `ProjectSnapshot`, `diff()`, `assert_equivalent()`, `DiffError`
- `diff/def_diff.rs` — slot-tree diff between parsed `NodeDef`s (kind preflight, map/option/enum/value ops)
- `diff/project_diff.rs` — path union diff: `.toml` → slot ops, assets → `SetBytes`/`Delete`
- `registry/slot_apply.rs` — `apply_ops_to_node_def`, project `nodes[*].def` SetSlot routing, record-map MapInsert fix, nested enum SetSlot

### Gate tests

`tests/project_diff.rs`:

- **A1** — `diff(∅, basic)` → apply → commit → equivalent
- **A1** — load_root roundtrip after blank compose
- **B1** — `diff(basic, basic2)` → apply → commit → equivalent
- identical snapshots → empty changeset

## Validation

```bash
cargo test -p lpc-node-registry --test project_diff
cargo test -p lpc-node-registry
cargo clippy -p lpc-node-registry --all-targets --no-deps -- -D warnings
```

## Known limits

- Equivalence uses parsed `NodeDef` equality (not byte-identical TOML)
- Slot diff verify compares serialized TOML (ignores revision metadata)
- Project node defs: path-backed invocations only in custom diff helper
- Inline child defs in project nodes fall back to nested slot diff
- Generic custom-slot diff incomplete beyond `NodeInvocation`

## Unblocks

Parent artifact-routed **M6 engine cutover** (ChangeSet apply in `lpc-engine`).
