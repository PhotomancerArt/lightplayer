# Phase 01 — Edit Types + Serde

**Dispatch:** sub-agent: yes | parallel: - | **Depends on:** M9 slot op names landed

## Scope of phase

Introduce `SlotEdit` and `AssetEdit`; rewrite `ArtifactEdit` as tagged union;
remove `EditOp`.

**In scope:**

- Create `slot_edit.rs`, `asset_edit.rs`
- Rewrite `artifact_edit.rs`
- Delete `edit_op.rs`
- Update `edit/mod.rs` exports and `lib.rs`
- Serde roundtrip tests (slot block, asset block, batch with both)
- Remove deprecated `ArtifactOp = EditOp` alias (or leave deprecated stub with note — prefer remove)

**Out of scope:** apply, diff, integration tests beyond unit serde.

## Code organization reminders

- One concept per file (`slot_edit.rs`, `asset_edit.rs`).
- `op_name()` on each op enum.
- `#[cfg(test)] mod tests` at bottom of `artifact_edit.rs` or `mod.rs`.

## Sub-agent reminders

- Do not commit.
- Do not touch `apply.rs`, `slot_apply.rs`, `def_diff.rs` yet — crate will not compile until phase 02/03; that's OK for this phase if executed alone, but prefer doing 01→02→03 in sequence on one branch.
- Fix warnings.

## Implementation details

### `slot_edit.rs`

Move slot variants from current `edit_op.rs`:

- `UseEnumVariant`, `AssignValue`, `MapInsert`, `MapRemove`, `UseOption`
- `impl SlotEdit { pub fn op_name(&self) -> &'static str }`

### `asset_edit.rs`

```rust
pub enum AssetEdit {
    Delete,
    ReplaceBody(String),
}
```

- `impl AssetEdit { pub fn op_name(&self) -> &'static str }` → `"delete"`, `"replace_body"`

### `artifact_edit.rs`

```rust
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArtifactEdit {
    Slot { target: EditTarget, ops: Vec<SlotEdit> },
    Asset { target: EditTarget, ops: Vec<AssetEdit> },
}
```

Add helpers:

```rust
impl ArtifactEdit {
    pub fn target(&self) -> &EditTarget { … }
    pub fn slot(target: EditTarget, ops: Vec<SlotEdit>) -> Self { … }
    pub fn asset(target: EditTarget, ops: Vec<AssetEdit>) -> Self { … }
}
```

### Exports (`edit/mod.rs`, `lib.rs`)

```rust
pub use asset_edit::AssetEdit;
pub use slot_edit::SlotEdit;
// remove EditOp export
```

### Tests

Replace invalid mixed-op roundtrip in `edit/mod.rs`:

- Test A: `ArtifactEdit::asset(Path("/shader.glsl"), [ReplaceBody(...)])`
- Test B: `ArtifactEdit::slot(Path("/shader.toml"), [UseEnumVariant(...)])`
- Test C: `EditBatch` with one slot + one asset edit

Assert JSON contains `"kind":"slot"` / `"kind":"asset"`.

## Validate

```bash
# After 02+03 land, full crate passes. For 01-only:
cargo test -p lpc-node-registry edit::tests
```

If crate fails to compile due to downstream references, note in handoff — expected until phase 02.
