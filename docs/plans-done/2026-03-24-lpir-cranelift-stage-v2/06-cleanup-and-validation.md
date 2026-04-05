## Scope of phase

- Grep diff for **`TODO`**, **`dbg!`**, stray **`eprintln!`**; remove or ticket.
- **Clippy / warnings:** `cargo clippy -p lp-glsl-filetests --all-features -D warnings`
  (adjust features to match crate).
- Ensure **`deny(missing_docs)`** satisfied for new public items (if any).

## Plan cleanup

- Add **`summary.md`** to this plan directory (completed work, deferred items,
  DEFAULT_TARGETS final policy).
- ~~After implementation merge, move this directory to **`docs/plans-done/`**~~ (done).

## Validate

```bash
cd /Users/yona/dev/photomancer/lp2025/lp-glsl && cargo test -p lp-glsl-filetests
cd /Users/yona/dev/photomancer/lp2025/lp-glsl && cargo clippy -p lp-glsl-filetests -- -D warnings
cargo +nightly fmt
```

## Commit

Conventional Commits, e.g.:

`feat(filetests): switch to lpir targets; use lp-glsl-exec stack`

Body: default `jit.q32`, CI wasm+rv32, remove legacy cranelift target, wire
filetests to **`lp-glsl-exec`** / **`lpvm`**, adapters, docs. Legacy
**`lp-glsl-frontend`** / **`lp-glsl-cranelift`** unchanged until Stage VII.
