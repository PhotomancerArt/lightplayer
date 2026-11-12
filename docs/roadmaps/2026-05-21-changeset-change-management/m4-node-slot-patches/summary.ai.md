# M4 Summary — Slot Ops + TOML Serialize

## Status

Implemented (`9234bb94`) on branch `codex/incremental-artifact-reload`.

Plan folder backfilled post-hoc; execution skipped numbered phase files.

## Delivered

### lpc-model

- `slot_mutation.rs` — `set_slot_value`, map/option remove helpers
- `slot_mut_access.rs` — `MapSlotMutAccess::remove_entry`, `SlotOptionMutAccess::clear_presence`

### lpc-node-registry

- `OverlayEntry::SlotDraft` + `change/slot_draft.rs`
- `registry/slot_apply.rs` — apply slot ops, fork draft, `serialize_slot_draft`
- `apply_change` / `apply_changeset` take `(fs, ctx, frame)`; route file vs slot ops
- `effective_read.rs` — slot draft projection + inline child `def_state_at_source`
- Inline child path routing — `entries[n].node.def.*` mutates invocation body

## Tests

`lp-core/lpc-node-registry/tests/slot_overlay.rs`:

- C1 — SetSlot patches clock rate in view; committed unchanged
- C1 — slot draft serializes to TOML (round-trip via `NodeDef::read_toml`)
- C2 — playlist parent slot patch; committed children unchanged
- C2 — inline child slot patch visible in view; committed child unchanged

Updated for new API: `overlay_lifecycle.rs`, `effective_projection.rs`, `asset_overlay.rs`.

## Validation

```bash
cargo test -p lpc-node-registry --test slot_overlay
cargo test -p lpc-node-registry
cargo test -p lpc-model slot_mutation
```

## Known limits (documented for M5/M6)

- New `.toml` paths fork from `NodeDef::default()` (Project) until kind SetSlot
- Custom slot paths (general `NodeInvocation` descent) use inline-specific router
- `MapInsert` / `MapRemove` / `OptionSet` implemented but not integration-tested
- Post-commit `NodeDefUpdates` for inline children — M5

## Next

M5 commit promotion — flush overlay to fs/store, re-derive entries, `SyncResult`.
