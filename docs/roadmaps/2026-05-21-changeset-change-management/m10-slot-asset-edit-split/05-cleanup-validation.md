# Phase 05 — Cleanup + Validation

**Dispatch:** main | parallel: - | **Depends on:** 04

## Scope of phase

Final pass: grep for stale symbols, fmt, clippy, summary.

**In scope:**

- `cargo +nightly fmt`
- Grep cleanup:
  - no `EditOp` in `lp-core/lpc-node-registry/`
  - no `SetBytes` in edit vocabulary (except comments/history if any)
  - no flat `ArtifactEdit { target, ops` struct literals
- `UnsupportedOp` — remove from `EditError` if dead
- Write `m10-slot-asset-edit-split/summary.md`
- Update `docs/roadmaps/.../changeset-change-management/summary.md` — M10 entry

**Out of scope:** wire crate, engine, partial diff implementation.

## Cleanup checklist

- [ ] `EditOp` deleted; not exported from `lib.rs`
- [ ] `ArtifactOp` deprecated alias removed or documents split
- [ ] Serde tests cover slot + asset kinds
- [ ] `apply_slot_op_on_non_toml_path_errors` still passes
- [ ] `project_diff` equivalence tests green
- [ ] Warnings fixed

## Validate

```bash
rustup update nightly
just check
cargo test -p lpc-node-registry
```

Optional (if touching nothing outside registry):

```bash
cargo test -p lpc-engine --lib
```

## Commit message (when user asks)

```
refactor(lpc-node-registry): split EditOp into SlotEdit and AssetEdit

- Tagged ArtifactEdit::{Slot, Asset} replaces flat ops list
- Rename SetBytes to AssetEdit::ReplaceBody
- Align apply/diff with overlay DefDraft vs Bytes split
```

## summary.md template

- Status: complete
- Delivered: typed split, apply/diff migration, docs
- Breaking: serde wire shape for `ArtifactEdit`; `EditOp` removed
- Validation commands
