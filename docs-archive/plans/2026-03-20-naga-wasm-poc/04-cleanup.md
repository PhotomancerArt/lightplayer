# Phase 4: Cleanup & validation

## Scope

Clean up any TODOs, debug prints, unused code. Verify everything compiles
cleanly and tests pass.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Cleanup

- Grep the git diff for any TODO, FIXME, debug println!, dbg!, etc. Remove
  or resolve them.
- Fix all warnings (`cargo check -p naga-wasm-poc` and
  `cargo clippy -p naga-wasm-poc` should be clean).
- Run `cargo +nightly fmt` on all changed files.

## Plan cleanup

Add a summary of findings to `docs/plans/2026-03-20-naga-wasm-poc/summary.md`:

- Did naga compile no_std?
- Did the GLSL → WASM path work?
- Did Q32 transform work?
- What surprised us?
- Recommendation: proceed with Naga, fork, or abandon?

Move plan files to `docs/plans-done/2026-03-20-naga-wasm-poc/`.

## Validate

```bash
cargo +nightly fmt -- --check
cargo clippy -p naga-wasm-poc
cargo test -p naga-wasm-poc
```

All must pass cleanly.

## Commit

```
spike(naga-wasm): POC for GLSL → Naga IR → WASM compilation path

- Spike crate at spikes/naga-wasm-poc/ validating Naga as GLSL frontend
- Float mode: GLSL → Naga IR → f32 WASM → wasmtime execution
- Q32 mode: same GLSL → i32 fixed-point WASM → wasmtime execution
- Validates Naga glsl-in compiles under #![no_std]
```
