# Phase 04 — Cleanup + Validation

**Dispatch:** sub-agent: main | parallel: -

## Scope of phase

Final polish for M1: docs, exports, clippy, remove stubs.

**In scope:**

- Update `change/mod.rs` module docs; remove "milestone M5" stub comment in any
  remaining files
- Ensure `lib.rs` exports are complete and documented
- `cargo +nightly fmt` on touched files
- Run full crate validation
- Add brief `summary.md` in this plan folder with validation commands + files
  touched

**Out of scope:** M2 projection work.

## Validate

```bash
cargo test -p lpc-node-registry
cargo test -p lpc-node-registry --test fs_change_semantics
cargo clippy -p lpc-node-registry --all-targets --no-deps -- -D warnings
```

If workspace gate needed before push:

```bash
just check  # optional; run if agent touches wider surface
```

## Milestone sign-off

- [ ] D1 test green
- [ ] D3 test green
- [ ] Serde round-trip tests green
- [ ] No `lpc-engine` changes
- [ ] Parent fs-change tests still green
