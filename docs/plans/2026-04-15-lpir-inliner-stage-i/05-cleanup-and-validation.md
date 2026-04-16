# Phase 5 — Cleanup, filetests, firmware check, summary

## Scope of phase

Remove `TODO` / stray debug, fix warnings introduced by the refactor, run full validation from M0 roadmap, write `summary.md`, and move this plan directory to `docs/plans-done/` per project convention. Optional: **commit** with Conventional Commits message when implementation is complete.

This phase is the **first gate** where the **entire workspace** touched by the refactor must be green (see commands below). Earlier phases may leave the build broken.

## Cleanup & validation

- Grep diff for `FIXME`, `TODO`, `dbg!`, `println!` used for debugging.
- Ensure no unused imports after renames (especially `FuncId` in cranelift files).
- **Full matrix:**

```bash
cargo test -p lpir
cargo test -p lpvm-native
cargo test -p lpvm-wasm
cargo test -p lpvm-cranelift
cargo test -p lpvm-emu
cargo test -p lps-frontend
cargo test -p lps-filetests -- --test-threads=4
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

Adjust paths if workspace uses different feature flags.

## Plan cleanup

- Add `summary.md` bullet list: what merged, crates touched, any follow-ups (e.g. M5 dead elim).
- Move `docs/plans/2026-04-15-lpir-inliner-stage-i/` → `docs/plans-done/2026-04-15-lpir-inliner-stage-i/` when work is complete.

## Commit (when requested)

```
refactor(lpir): stable CalleeRef with ImportId and FuncId

- Replace flat CalleeRef(u32) with enum Import/Local
- Store local functions in BTreeMap<FuncId, IrFunction>
- Update backends and frontend for new module layout
```
