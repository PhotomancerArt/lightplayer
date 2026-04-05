# Phase 5: Cleanup and validation

## Cleanup & validation

- Grep for **`TODO`**, **`dbg!`**, temporary **`@unimplemented`** left without reason.
- Run **`cargo +nightly fmt`** on touched Rust.
- **`cargo test -p lps-naga`**
- **`cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server`**
- **`cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu`** (per `AGENTS.md` when `lps` / `lp-fw` touched)
- Fix **warnings** introduced in this work.

## Plan cleanup

- Finalize **[`summary.md`](./summary.md)** with bullets: what shipped, Tier B deferrals, trap follow-up if any.
- Per workspace plan convention: when the implementation is **merged and verified**, move this directory to **`docs/plans-done/`** (coordinate with the user if the move should happen in the same PR as code).

## Commit

Conventional commit, e.g.:

```
feat(lps-naga): lower Access for vector/matrix load/store

- Select-chain dynamic index for vec2–4 / ivec / bvec
- Store through Access; matrix column and inc/dec patterns
- Unmark Milestone II filetests; document trap gaps in summary
```
