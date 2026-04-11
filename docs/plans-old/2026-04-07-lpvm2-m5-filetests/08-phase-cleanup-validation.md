# Phase 8: Cleanup, validation, plan closure

## Scope of phase

- Grep for `TODO`, `FIXME`, `unimplemented!`, `dbg!`, stray `println!` in the M5 diff.
- **`cargo +nightly fmt`** on touched crates.
- Fix all **warnings** introduced or left unresolved.
- Full **filetest matrix** (same as CI): `jit.q32`, `jit.f32`, `rv32.q32`, `rv32.f32`, `wasm.q32`, `wasm.f32` as applicable.

## Plan cleanup

- Write **`summary.md`** in this directory: what shipped, notable API additions (`call_q32`, `debug_state`), lifecycle change (engine/module per file).
- Move **`docs/plans/2026-04-07-lpvm2-m5-filetests/`** → **`docs/plans-done/`** (entire folder).

## Commit

Conventional commit, e.g.:

```
feat(filetests): migrate to LpvmEngine and add call_q32

- Add LpvmInstance::call_q32 and debug_state
- Engine/module per filetest file; instance per case
- Remove GlslExecutable from lps-filetests
```

## Validate

```bash
cargo +nightly fmt
just fci-glsl
# or minimal:
cargo check -p lpvm -p lpvm-cranelift -p lpvm-emu -p lpvm-wasm -p lps-filetests
cargo test -p lps-filetests
```
