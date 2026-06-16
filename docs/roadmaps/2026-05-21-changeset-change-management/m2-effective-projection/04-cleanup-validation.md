# Phase 04 — Cleanup + Validation

**Dispatch:** sub-agent: main | parallel: -

## Scope of phase

Docs, fmt, clippy, milestone summary.

**In scope:**

- `summary.md` in this plan folder
- Module docs on `effective_read`, `NodeDefView`
- `cargo +nightly fmt`
- Full crate validation

**Out of scope:** M3 materialize overlay.

## Validate

```bash
cargo test -p lpc-node-registry
cargo test -p lpc-node-registry --test fs_change_semantics
cargo test -p lpc-node-registry --test overlay_lifecycle
cargo test -p lpc-node-registry --test effective_projection
cargo clippy -p lpc-node-registry --all-targets --no-deps -- -D warnings
```

## Milestone sign-off

- [ ] `read_effective_bytes` overlay precedence
- [ ] `NodeDefView::get` effective only (requires fs + ctx)
- [ ] Committed `entries` unchanged on apply (regression)
- [ ] D1 view-vs-committed test green
