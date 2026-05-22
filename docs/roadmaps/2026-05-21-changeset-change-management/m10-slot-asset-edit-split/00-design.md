# M10 Design — SlotEdit / AssetEdit Split

## Scope

Replace monolithic `EditOp` with two op enums and a tagged `ArtifactEdit`. Align
types with overlay storage (`DefDraft` vs `Bytes` / `Deleted`) before wire or
client work builds on the old shape.

**In:** `lpc-node-registry` edit module, apply, diff, tests, change-language docs.

**Out:** Partial text diffs, `lpc-wire`, serde compat shims for flat `{ target, ops }`.

## File structure

```text
lp-core/lpc-node-registry/src/edit/
├── mod.rs                 # exports SlotEdit, AssetEdit, ArtifactEdit, …
├── slot_edit.rs           # NEW — structured slot mutations
├── asset_edit.rs          # NEW — path-level file body ops
├── artifact_edit.rs       # REWRITE — tagged Slot | Asset
├── edit_batch.rs          # unchanged shape: Vec<ArtifactEdit>
├── edit_target.rs
├── edit_error.rs          # drop UnsupportedOp for cross-kind misuse (keep for unknown variants if needed)
├── apply.rs               # match ArtifactEdit::Asset only
├── slot_overlay.rs
├── def_draft.rs
└── edit_op.rs             # DELETE

lp-core/lpc-node-registry/src/
├── registry/
│   ├── slot_apply.rs      # SlotEdit only; remove asset reject arm
│   └── node_def_registry.rs  # match ArtifactEdit::{Slot, Asset}
└── diff/
    ├── def_diff.rs        # -> Vec<SlotEdit>
    └── project_diff.rs    # -> ArtifactEdit::Slot | Asset
```

## Types

### `SlotEdit`

Slot-tree mutations within a `.toml` artifact (unchanged semantics from current
slot half of `EditOp`):

```rust
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotEdit {
    UseEnumVariant { path: SlotPath, variant: String },
    AssignValue { path: SlotPath, value: LpValue },
    MapInsert { path: SlotPath, key: String, value: LpValue },
    MapRemove { path: SlotPath, key: String },
    UseOption { path: SlotPath, present: bool },
}
```

Each variant gets `op_name()` for logging/errors (same strings as today).

### `AssetEdit`

Path-level committed overlay state (whole artifact):

```rust
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetEdit {
    Delete,
    ReplaceBody(String),   // was SetBytes
}
```

### `ArtifactEdit`

```rust
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArtifactEdit {
    Slot { target: EditTarget, ops: Vec<SlotEdit> },
    Asset { target: EditTarget, ops: Vec<AssetEdit> },
}
```

Optional convenience constructors (same file or `impl ArtifactEdit`):

```rust
impl ArtifactEdit {
    pub fn slot(target: EditTarget, ops: Vec<SlotEdit>) -> Self { … }
    pub fn asset(target: EditTarget, ops: Vec<AssetEdit>) -> Self { … }
}
```

### Removed

- `EditOp` — delete; no type alias (ambiguous union).
- Deprecated `ArtifactOp = EditOp` — remove or repoint doc to `SlotEdit`/`AssetEdit`.

## Apply flow

```text
EditBatch.edits[]
       │
       ▼
apply_artifact_edit(edit)
       │
       ├─ ArtifactEdit::Asset { target, ops }
       │     resolve target → path
       │     for op in ops: apply_asset_op(overlay, path, op)
       │         Delete      → overlay.apply_delete
       │         ReplaceBody → overlay.apply_bytes
       │
       └─ ArtifactEdit::Slot { target, ops }
             resolve target → path
             ensure .toml (existing ensure_toml_path)
             fork DefDraft → apply each SlotEdit → write DefDraft back
```

No runtime `UnsupportedOp` for “slot op on asset path” — wrong `kind` is a
client/authoring mistake caught at construction or (for `.glsl` + `Slot`) at
existing `ensure_toml_path`.

## Diff flow

| Path kind | Output |
|-----------|--------|
| `.toml` content changed | `ArtifactEdit::Slot { ops: diff_node_defs(...) }` |
| non-`.toml` added/changed | `ArtifactEdit::Asset { ops: [ReplaceBody(text)] }` |
| any path removed | `ArtifactEdit::Asset { ops: [Delete] }` |

`diff_node_defs` return type: `Vec<SlotEdit>`.

## Serde / wire

Breaking change from flat struct:

```json
// before
{ "target": { "path": "/a.glsl" }, "ops": [{ "set_bytes": "…" }] }

// after
{ "kind": "asset", "target": { "path": "/a.glsl" }, "ops": [{ "replace_body": "…" }] }
```

Acceptable: no production wire consumers yet.

## Documentation updates

- [`change-language.md`](../change-language.md) — two op tables, tagged `ArtifactEdit`, examples
- [`vocabulary.md`](../m8-edit-session-sync/vocabulary.md) — add M10 row: `EditOp` → `SlotEdit` + `AssetEdit`
- [`overview.md`](../overview.md) — summary diagram snippet

## Validation

```bash
cargo test -p lpc-node-registry
cargo check -p lpc-node-registry --no-default-features  # embedded / no diff
cargo test -p lpc-node-registry --features diff
```

Host CI gate when committing: `just check` + `just test`.

## Phase map

| Phase | Title | Depends |
|-------|-------|---------|
| 01 | Edit types + serde | — |
| 02 | Apply pipeline | 01 |
| 03 | Diff + project_diff | 01 |
| 04 | Integration tests + docs | 02, 03 |
| 05 | Cleanup + validation | 04 |

Phases 02 and 03 can run in parallel after 01 (disjoint files).
