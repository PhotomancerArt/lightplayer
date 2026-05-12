# Phase 5: Cleanup & validation

## Scope of phase

Final sweep: remove temporary code, fix warnings, verify formatting, write summary, archive plan.

## Cleanup

Grep the git diff for:

- `TODO` comments — resolve or document as intentional
- `dbg!`, `println!`, `eprintln!` in non-test code — remove
- `#[allow(dead_code)]` added during development — remove if the code is no longer dead
- `#[ignore]` on tests that now pass — remove (especially `test_q32_float_mul`)

## Validate

```bash
cargo build
cargo test
cargo +nightly fmt --check
./scripts/filetests.sh --target wasm.q32
./scripts/filetests.sh --target cranelift.q32
```

All must pass. No new warnings in touched crates.

## Plan cleanup

Add `summary.md` to this directory with:

- What shipped (bullet list)
- Known limitations
- Follow-ups (e.g. memory layout growth, browser playground, matrix builtins)

**Done:** `docs/plans-done/2026-03-19-glsl-wasm-builtins2/` (includes `summary.md`). Predecessor plan: `docs/plans-done/2026-03-18-glsl-wasm-builtins/`.

## Commit

```
feat(wasm): LPFX builtins, out params, and Rainbow shader support

- Fix psrdnoise seed parameter bug (add seed to GLSL sig, fix Cranelift registry)
- Add inline floor and fract for Q32 fixed-point
- Implement LPFX call emission (worley, fbm, psrdnoise) with flattened args
- Add out-parameter support via shared linear memory (static offset 0)
- Update filetest runner with builtins.wasm linking
- Rainbow shader compiles and runs under wasmtime
```
