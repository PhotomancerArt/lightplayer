# Phase 03 — D2 Harness (Commit Updates Base)

**Dispatch:** sub-agent: yes | parallel: -

## Scope of phase

Integration tests proving **D2**: commit updates committed cache and clears overlay.

**In scope:**

- `tests/commit_promotion.rs` (new) or extend `overlay_lifecycle.rs` — prefer dedicated file
- Tests:
  1. **`d2_commit_updates_committed_and_clears_overlay`** — `load_clock`, SetSlot
     `controls.rate = 2.0`, commit, assert `get()` rate 2.0, `!overlay_active()`
  2. **`d2_commit_setbytes_updates_committed`** — SetBytes on `/clock.toml`, commit,
     committed matches overlay
  3. **`d2_commit_writes_slot_draft_to_fs`** — after SetSlot + commit, read fs file,
     contains `rate = 2` (or equivalent)

**Out of scope:** D5, inline child SyncResult (phases 04–05).

## Sub-agent reminders

- Do not commit.
- Use `fixtures::load_clock`, existing `apply_change` helpers from sibling tests.

## Validate

```bash
cargo test -p lpc-node-registry --test commit_promotion
cargo test -p lpc-node-registry
```
