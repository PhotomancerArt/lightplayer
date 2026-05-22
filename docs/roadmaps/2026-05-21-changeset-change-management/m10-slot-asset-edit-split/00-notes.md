# M10 Notes — SlotEdit / AssetEdit Split

## Scope

Split the monolithic `EditOp` enum into **`SlotEdit`** and **`AssetEdit`**, and make
`ArtifactEdit` a **tagged union** so each artifact block is either slot-structured
or opaque-file, never both.

**In:** `lpc-node-registry` edit types, apply/diff, in-crate tests, `change-language.md`,
`vocabulary.md`.

**Out:** `lpc-wire` / client protocol, partial text diff ops, engine cutover.

## Current state

Layer-1 edit vocabulary lives in `lpc-node-registry/src/edit/`:

| File | Role |
|------|------|
| `edit_op.rs` | Single `EditOp` mixing slot + asset variants |
| `artifact_edit.rs` | `{ target, ops: Vec<EditOp> }` |
| `apply.rs` | Handles `Delete` / `SetBytes`; rejects slot ops |
| `registry/slot_apply.rs` | Handles slot ops; rejects `Delete` / `SetBytes` |
| `registry/node_def_registry.rs` | Dispatches per-op in `apply_artifact_edit` |

Overlay storage already mirrors the split:

```text
SlotOverlayEntry = Deleted | Bytes | DefDraft
```

`def_diff` emits only slot ops. `project_diff` emits slot ops for `.toml` and
`Delete` / `SetBytes` for other paths — but types allow invalid mixes (e.g. serde
roundtrip test combines `SetBytes` + `UseEnumVariant` on one block).

Recent naming (M9 follow-up, uncommitted or landing soon):

- `UseEnumVariant`, `AssignValue`, `UseOption` (slot)
- `Delete`, `SetBytes` (asset)

## User intent

- Split **before** building more on the edit model.
- Names **`SlotEdit`** and **`AssetEdit`**.
- Asset side should evolve independently (future partial text diffs); not implemented now.
- Prefer language without “set” on slot ops (already done).

## Open questions

### Q1 — Serde wire shape for `ArtifactEdit`

**Context:** Today `ArtifactEdit` is a struct `{ target, ops }`. Mixed op lists
deserialize even when invalid at apply time.

**Suggested answer:** Externally tagged union:

```rust
#[serde(tag = "kind", rename_all = "snake_case")]
enum ArtifactEdit {
    Slot { target: EditTarget, ops: Vec<SlotEdit> },
    Asset { target: EditTarget, ops: Vec<AssetEdit> },
}
```

Wire example:

```json
{ "kind": "slot", "target": { "path": "/shader.toml" }, "ops": [ … ] }
{ "kind": "asset", "target": { "path": "/shader.glsl" }, "ops": [ … ] }
```

**Status:** confirmed.

### Q2 — Rename `SetBytes` → `ReplaceBody` on asset side?

**Context:** User suggested `ReplaceBody` when discussing the split; clearer for
whole-file replacement and distinct from future patch ops.

**Suggested answer:** Yes, rename in this milestone (`AssetEdit::ReplaceBody`).

**Status:** confirmed.

### Q3 — Backward-compat for old `EditOp` / flat `ArtifactEdit` serde?

**Context:** M8 vocabulary renamed types; wire not shipped to production clients.
Harness tests are the only consumers.

**Suggested answer:** Break wire shape cleanly. Remove `EditOp`. Keep deprecated
`ArtifactOp` alias only if we add a shim — prefer **no shim**; update
`#[deprecated] pub type ArtifactOp` doc to point at `SlotEdit | AssetEdit`.

**Status:** confirmed — no serde shim.

### Q4 — `AssetEdit` op list: `Vec` vs single op?

**Context:** Today asset blocks usually have one op (`Delete` or `ReplaceBody`).
Slot blocks often have many.

**Suggested answer:** `Vec<AssetEdit>` for both variants — consistent, and leaves
room for ordered multi-step asset edits without another shape change.

**Status:** confirmed.

## Dependencies

- M9 slot op naming should land first (or in same branch) so phase 01 starts from
  `UseEnumVariant` / `AssignValue` / `UseOption`, not legacy `VariantSet` / `SetSlot`.

## Risks

- Mechanical churn across ~10 test files and diff helpers — low logic risk.
- `UnsupportedOp` paths should disappear; replace with compile-time enforcement.
- `apply_slot_op_on_non_toml_path_errors` remains valid via `ArtifactEdit::Slot` on
  a `.glsl` path.
