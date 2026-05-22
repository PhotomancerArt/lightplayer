# Phase 04 — Integration Tests + Docs

**Dispatch:** sub-agent: yes | parallel: - | **Depends on:** 02, 03

## Scope of phase

Update all harness/integration tests and canonical change-language docs.

**In scope:**

- Tests under `lpc-node-registry/tests/`:
  - `asset_overlay.rs`
  - `commit_promotion.rs`
  - `effective_projection.rs`
  - `overlay_lifecycle.rs`
  - `pending_sync.rs`
  - `slot_overlay.rs`
- `change-language.md`
- `m8-edit-session-sync/vocabulary.md` (M10 table)
- `overview.md` change-language summary (optional one-liner)

**Out of scope:** archived plan docs, M9 phase files (historical).

## Sub-agent reminders

- Do not commit.
- Use `ArtifactEdit::slot(...)` / `::asset(...)` helpers for readability.
- Fix test names/comments referencing `SetBytes` → `ReplaceBody` where user-facing.

## Implementation details

### Test migration pattern

```rust
// before
ArtifactEdit {
    target: EditTarget::Path(...),
    ops: vec![EditOp::SetBytes("…".into())],
}

// after
ArtifactEdit::asset(
    EditTarget::Path(...),
    vec![AssetEdit::ReplaceBody("…".into())],
)
```

```rust
// before
ArtifactEdit { target, ops: vec![EditOp::AssignValue { … }] }

// after
ArtifactEdit::slot(target, vec![SlotEdit::AssignValue { … }])
```

```rust
// Delete
ArtifactEdit::asset(target, vec![AssetEdit::Delete])
```

### Imports

Replace `EditOp` with `SlotEdit`, `AssetEdit` in test use lines.

### `change-language.md`

Rewrite ops section:

```rust
enum ArtifactEdit {
    Slot { target: EditTarget, ops: Vec<SlotEdit> },
    Asset { target: EditTarget, ops: Vec<AssetEdit> },
}
```

Two op tables (slot vs asset). Update all examples (including creatability + add-shader example).

### `vocabulary.md`

Add section:

| Old | New |
|-----|-----|
| `EditOp` | removed — use `SlotEdit` or `AssetEdit` |
| flat `ArtifactEdit { ops }` | tagged `ArtifactEdit::Slot` / `::Asset` |
| `SetBytes` | `AssetEdit::ReplaceBody` |

## Validate

```bash
cargo test -p lpc-node-registry
```

All tests must pass.
