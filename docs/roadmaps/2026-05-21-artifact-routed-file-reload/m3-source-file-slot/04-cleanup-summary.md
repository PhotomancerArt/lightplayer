# Phase 04 — Cleanup + summary

**Dispatch:** [sub-agent: no, supervised]

## Scope of phase

Finalize M3 exports, formatting, clippy, and milestone summary.

**In scope:**

- `lpc-node-registry/src/lib.rs` re-exports for source API
- `cargo +nightly fmt --all`
- Clippy on touched crates
- `summary.md` with decisions for M4/M6
- Confirm production `ShaderSource` / `lpc-engine` untouched

**Out of scope:** M4 harness, M6 cutover.

## Validate

```bash
cargo +nightly fmt --all
cargo test -p lpc-model source_file
cargo test -p lpc-node-registry
cargo clippy -p lpc-node-registry --all-targets --no-deps -- -D warnings
cargo clippy -p lpc-model --all-targets --no-deps -- -D warnings
```

## Summary checklist

- [x] `SourceFileSlot` codec round-trips
- [x] `resolve_source_file` acquires file artifacts
- [x] `materialize_source` combines revisions correctly
- [x] File bump test passes without def TOML change
- [x] No edits under `lpc-engine`
