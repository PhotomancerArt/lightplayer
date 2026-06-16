# Phase 03 — Diff + project_diff

**Dispatch:** sub-agent: yes | parallel: 02 | **Depends on:** 01

## Scope of phase

Update diff helpers to emit `SlotEdit` and tagged `ArtifactEdit`.

**In scope:**

- `diff/def_diff.rs` — return `Vec<SlotEdit>`; update `push_*` helpers
- `diff/project_diff.rs` — wrap results in `ArtifactEdit::Slot` / `::Asset`
- `registry/slot_apply.rs` — `apply_ops_to_node_def` takes `&[SlotEdit]` (if not done in 02)

**Out of scope:** integration tests (phase 04), docs.

## Sub-agent reminders

- Do not commit.
- Preserve diff verify loop (apply ops to clone, compare TOML).
- Empty slot diff → omit `ArtifactEdit` entry (unchanged behavior).

## Implementation details

### `def_diff.rs`

```rust
pub fn diff_node_defs(...) -> Result<Vec<SlotEdit>, DiffError>
```

- All `EditOp::…` → `SlotEdit::…`
- `ops: &mut Vec<SlotEdit>` in internal helpers

### `project_diff.rs`

```rust
(Some(_), None) => changes.push(ArtifactEdit::asset(
    EditTarget::Path(...),
    vec![AssetEdit::Delete],
)),

// .toml
changes.push(ArtifactEdit::slot(
    EditTarget::Path(...),
    ops,
));

// non-.toml
changes.push(ArtifactEdit::asset(
    EditTarget::Path(...),
    vec![AssetEdit::ReplaceBody(text)],
));
```

Remove `EditOp` import.

### `def_diff` unit test

`diff_shader_from_default` — update assertions if op type names changed only.

## Validate

```bash
cargo test -p lpc-node-registry diff::
cargo test -p lpc-node-registry --test project_diff
```

Integration tests outside `diff` module may fail until phase 04.
