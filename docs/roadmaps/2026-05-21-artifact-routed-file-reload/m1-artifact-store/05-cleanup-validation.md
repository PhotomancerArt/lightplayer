# Phase 05 — Cleanup and validation

**Dispatch:** [sub-agent: supervised, model: composer-2.5-fast, parallel: -]

## Scope of phase

- Re-export primary artifact types from `lib.rs` for downstream crates.
- Grep diff for debug prints, stray TODOs, `content_frame` naming.
- Run full validation commands.
- Write `summary.md` per plan process.

**Out of scope:** New features, M2 work.

## Code Organization Reminders

- `lib.rs` re-exports: `ArtifactStore`, `ArtifactId`, `ArtifactLocation`,
  `ArtifactError`, `ArtifactReadState`, `ArtifactEntry`.

## Sub-agent Reminders

- Do **not** commit (plan commits once at end).
- Fix all clippy warnings in `lpc-node-registry`.

## Implementation Details

### `lib.rs` re-exports

```rust
pub use artifact::{
    ArtifactEntry, ArtifactError, ArtifactId, ArtifactLocation, ArtifactReadState,
    ArtifactStore,
};
```

### Update milestone cross-reference (optional doc touch)

If `docs/roadmaps/.../m1-artifact-store.md` still says `content_frame`, update to
`revision` and requester-owned acquire/release (only if editing docs in this phase).

### `summary.md`

Create plan summary with **What was built** and **Decisions for future reference**
(ownership model, revision naming, fs does not register, release-at-zero removes entry).

### Grep checks

```bash
rg "content_frame" lp-core/lpc-node-registry/
rg "TODO|dbg!|println!" lp-core/lpc-node-registry/
rg "lpc-slot-mockup" Cargo.toml lp-core/
```

Fix any findings.

## Validate

```bash
cargo +nightly fmt --all
cargo test -p lpc-node-registry
cargo clippy -p lpc-node-registry --all-targets -- -D warnings
cargo check -p lpc-node-registry --no-default-features
```

All must pass.
