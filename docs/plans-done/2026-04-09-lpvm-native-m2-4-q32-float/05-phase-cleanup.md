# Phase 5: Cleanup, validation, summary, and commit

## Scope of phase

Remove stray TODOs/debug prints, run full validation, write `summary.md`, move the plan to `docs/plans-done/`, and commit with Conventional Commits.

## Code Organization Reminders

- Grep the diff for `TODO`, `dbg!`, `println!` in touched code.
- Fix warnings introduced by this work; do not leave “fix later” unless a follow-up plan owns it.

## Implementation Details

1. **Repo hygiene**
   - `rg 'TODO|dbg!|println!' lp-shader/lpvm-native/src/lower.rs` (and any other touched files).

2. **Validation** (per AGENTS.md for shader path touches)

   ```bash
   cargo test -p lpvm-native --lib
   cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
   ```

   Optional broader check:

   ```bash
   cargo test -p lps-filetests --test rv32lp_smoke
   ```

3. **Formatting**

   ```bash
   cargo +nightly fmt -p lpvm-native
   ```

4. **Plan wrap-up**
   - Add `summary.md` under this plan directory: what shipped, files touched, validation commands run.
   - Move `docs/plans/2026-04-09-lpvm-native-m2-4-q32-float/` → `docs/plans-done/` (entire folder).

5. **Commit**

   ```
   feat(lpvm-native): Q32 fdiv and float compares in lower_op

   - Lower Fdiv to __lp_lpir_fdiv_q32; Feq..Fge to Icmp32 (match cranelift Q32)
   - Single F32-mode error for all float ops; extend tests
   ```

   Adjust body bullets to match the actual diff.

## Validate

Same commands as in Implementation Details; all must pass before commit.
