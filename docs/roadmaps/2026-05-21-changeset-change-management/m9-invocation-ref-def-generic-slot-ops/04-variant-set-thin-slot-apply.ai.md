# Phase 04 — VariantSet + Thin slot_apply

**Dispatch:** sub-agent: yes | parallel: - | **Depends on:** 03

## Scope of phase

Add **`VariantSet`** to edit vocabulary and reduce `slot_apply.rs` to generic slot
mutation only.

**In scope:**

- `edit/edit_op.rs` — add `VariantSet { path, variant }`
- `registry/slot_apply.rs` — dispatch table; delete shortcuts
- `edit/mod.rs` serde roundtrip test

**Delete from `slot_apply.rs`:**

- `apply_set_slot_on_def` string→variant heuristic
- `project_node_def_mutation`
- `apply_node_invocation_def`
- `inline_body_mutation` / `matching_inline_inner_path` / `invocation_at_mut` (if generic paths work)

**Keep:**

- `apply_op_to_def` match arms for Map/Option
- `mutate_def` wrapper
- `ensure_toml_path`, `parse_def_bytes`, `serialize_slot_draft`

## EditOp

```rust
VariantSet { path: SlotPath, variant: String },
SetSlot { path: SlotPath, value: LpValue },  // doc: value leaves only
```

`op_name()` → `"variant_set"`.

## Apply

```rust
EditOp::VariantSet { path, variant } => mutate_def(def, |root| {
    set_slot_variant_default(root, ctx.shapes, path, frame, variant)
}),
EditOp::SetSlot { path, value } => mutate_def(def, |root| {
    set_slot_value(root, ctx.shapes, path, frame, value.clone())
}),
```

If generic mutation fails on paths that worked via shortcuts, **fix slot shapes in
phase 01/03** — do not re-add registry routers.

## Tests

Update hand-written ops in tests still using `SetSlot + String` for kind/wiring:

- `slot_overlay.rs`, `commit_promotion.rs`, `pending_sync.rs`, etc.

Use `VariantSet` for kind changes; `VariantSet(…, "Ref")` + `SetSlot(…ref, …)` for wiring.

## Validate

```bash
cargo test -p lpc-node-registry --test slot_overlay --test overlay_lifecycle
cargo test -p lpc-node-registry
```

Diff may still emit old ops — phase 05.
