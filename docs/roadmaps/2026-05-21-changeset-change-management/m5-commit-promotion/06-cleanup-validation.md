# Phase 06 — Cleanup + Validation

**Dispatch:** sub-agent: main | parallel: -

## Scope of phase

Final polish for M5: docs, exports, fmt, full validation, milestone summary.

**In scope:**

- `cargo +nightly fmt` on touched Rust files
- Update `change/mod.rs` / registry module docs referencing commit
- `summary.md` in this plan folder (status, deliverables, validation commands)
- Fix clippy warnings in touched code
- Confirm no `lpc-engine` changes

**Out of scope:** M6 diff work, M3/M4 backfill beyond existing `summary.md`.

## Milestone sign-off

- [ ] D2 tests green
- [ ] D5 tests green
- [ ] C2 post-commit test green
- [ ] All existing `lpc-node-registry` tests green
- [ ] `commit-contract.md` matches implementation

## Validate

```bash
cargo test -p lpc-node-registry
cargo test -p lpc-node-registry --test commit_promotion
cargo test -p lpc-node-registry --test overlay_lifecycle
cargo test -p lpc-node-registry --test slot_overlay
cargo test -p lpc-node-registry --test asset_overlay
cargo test -p lpc-node-registry --test fs_change_semantics
cargo clippy -p lpc-node-registry --all-targets --no-deps -- -D warnings
```

Optional before push: `just check`
