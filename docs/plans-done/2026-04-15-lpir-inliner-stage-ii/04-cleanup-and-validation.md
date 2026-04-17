# Phase 4 — Cleanup & validation

## Scope of phase

- Grep the working tree for **`TODO`**, **`FIXME`**, stray **`dbg!`**, debug **`println!`** introduced during this plan.
- Fix warnings (unused imports after plumbing, **`dead_code`** only if legitimately unused stubs — prefer **`allow`** with a one-line reason or remove).
- Run the **full M1 validation matrix** from the roadmap.

## Cleanup & validation

```bash
cargo test -p lpir
cargo test -p lpvm-native
cargo test -p lpvm-cranelift
cargo test -p lpvm-wasm
cargo test -p lps-filetests -- --test-threads=4
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
  --profile release-esp32 --features esp32c6,server
```

Add **`cargo check -p fw-emu`** or **`lp-server`** if this workspace’s AGENTS checklist applies to these crates.

## Plan cleanup

- Write **`docs/plans/2026-04-15-lpir-inliner-stage-ii/summary.md`**: bullets — what shipped (`CompilerConfig`, three backends, **`compile-opt`** parsing + harness), crates touched, follow-ups (M4 inliner reads **`InlineConfig`**; tag **`filetests/function/*.glsl`**).
- Move **`docs/plans/2026-04-15-lpir-inliner-stage-ii/`** → **`docs/plans-done/2026-04-15-lpir-inliner-stage-ii/`** when implementation is complete.

## Commit (when requested)

Conventional Commits example:

```
feat(lpir): add CompilerConfig and filetest compile-opt directive

- Add CompilerConfig / InlineConfig / InlineMode in lpir
- Thread config through native, Cranelift, and WASM compile options
- Parse // compile-opt(key, value) into TestFile and apply before compile
```

## Code Organization Reminders

- Final pass: no temporary hacks without **`TODO(plan):`** if something must remain.
