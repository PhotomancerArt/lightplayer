# Phase 7: Rainbow shader, filetests, cleanup, docs

## Scope of phase

- Compile **`examples/.../rainbow.shader/main.glsl`** (or canonical path) via `glsl_wasm` + full linking; fix remaining gaps.
- Add or extend **integration test**: deterministic pixel/vec outputs vs Cranelift for a few inputs (if policy allows comparison), or snapshot hashes.
- **Filetests:** Remove `@unimplemented(backend=wasm)` where wasm.q32 passes; run broader `./scripts/glsl-filetests.sh --target wasm.q32` and ensure **cranelift.q32** unchanged.
- **Docs:** Update `lp-glsl-wasm/README.md`, `lp-glsl-filetests/README.md`, `impl-notes.md` — document builtins.wasm build, memory import, linking order.
- **Plan:** Write `summary.md`, move plan directory to `docs/plans-done/` per project convention.

## Code organization reminders

- Final grep for `TODO`, `dbg!`, `println!` in touched areas.

## Implementation details

- `test_q32_float_mul`: remove `#[ignore]` if not already done.
- `just build-fw-esp32` if required by repo policy after large changes.

## Validate

```bash
cargo build
cargo test
cargo +nightly fmt --check
./scripts/glsl-filetests.sh --target wasm.q32
./scripts/glsl-filetests.sh --target cranelift.q32
just build-fw-esp32   # if applicable
```

## Plan cleanup

- Add `summary.md` with completed items and follow-ups (e.g. texture upload offsets).
- Archived at `docs/plans-done/2026-03-18-glsl-wasm-builtins/`.

## Commit

Conventional commit after full validation, e.g. `feat(wasm): builtin imports and builtins.wasm linking`.
