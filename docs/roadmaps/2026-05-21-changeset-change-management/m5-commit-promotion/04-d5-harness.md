# Phase 04 — D5 Harness (Overlay vs Fs Precedence)

**Dispatch:** sub-agent: yes | parallel: -

## Scope of phase

Integration tests proving **D5** precedence from `commit-contract.md`.

**In scope:**

- Add to `tests/commit_promotion.rs`:
  1. **`d5_overlay_wins_over_stale_fs`** — load clock, apply SetSlot (overlay rate
     2.0), write fs with rate 9.0 directly, assert `view()` still 2.0, `get()` still 1.0
  2. **`d5_sync_fs_does_not_clobber_overlay_view`** — with overlay active, call
     `sync_fs` on same path; assert view still overlay value
  3. **`d5_post_commit_fs_sync_updates_committed`** — commit overlay, then fs write +
     `sync_fs`; assert committed updates to fs value

**Out of scope:** compose-from-blank, engine.

## Sub-agent reminders

- Do not commit.
- Document in test names what each step proves (overlay > fs pre-commit).

## Validate

```bash
cargo test -p lpc-node-registry --test commit_promotion
cargo test -p lpc-node-registry
```
