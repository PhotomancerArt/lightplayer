# Phase 1: Verify notify Dependency

## Goal

Verify that the `notify` crate dependency is available and ready to use.

## Steps

1. **Check existing dependency**
   - File: `lp-app/apps/lp-cli/Cargo.toml`
   - Verify `notify = "6"` is present (it already is)

2. **Verify dependency works**
   - Run `cargo build` to ensure it compiles
   - Check that no conflicts arise

## Verification

- [x] `notify` crate is already in dependencies (v6)
- [ ] `cargo build -p lp-cli` succeeds
- [ ] `notify` crate is available in the project

## Notes

- `notify` v6.x supports async via `tokio` integration
- We'll use the async API for integration with `fs_loop`
- No changes needed - dependency already exists!
