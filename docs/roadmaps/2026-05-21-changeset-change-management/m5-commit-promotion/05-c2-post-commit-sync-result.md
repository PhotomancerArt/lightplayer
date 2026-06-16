# Phase 05 — C2 Post-Commit SyncResult

**Dispatch:** sub-agent: yes | parallel: -

## Scope of phase

Verify inline child edits appear in `SyncResult.def_updates` after commit (M4
deferral documented in `m4-node-slot-patches/summary.md`).

**In scope:**

- Add to `tests/commit_promotion.rs`:
  - **`c2_inline_child_changed_after_commit`** — `load_playlist_with_inline_child`,
    apply SetSlot on `entries[2].node.def.render_order = 7`, commit, assert:
    - child id in `result.def_updates.changed`
    - root **not** in changed (mirror `fs_change_semantics` S4 pattern)
    - committed child render_order is 7

Optional:

- Asset commit bumps `source_revisions` (C4c post-commit) if straightforward via
  existing `sync_source_path`.

**Out of scope:** full C4 matrix, A1 compose.

## Sub-agent reminders

- Do not commit.
- Reuse `inline_child_id` helper pattern from `slot_overlay.rs` / `fs_change_semantics.rs`.

## Validate

```bash
cargo test -p lpc-node-registry --test commit_promotion
cargo test -p lpc-node-registry --test fs_change_semantics
```
