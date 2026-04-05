# Phase 6: Cleanup, validation, handoff

## Scope of phase

Remove stray TODOs, run full formatting/lint for touched crates, add **`summary.md`**, move plan to
**`docs/plans-done/`**, conventional commit.

## Cleanup & validation

- Grep **`pp-rs`** and **`lp2025`** diff for **`TODO`**, **`dbg!`**, **`println!`**, temporary *
  *`path`** patches left enabled by mistake.
- Run **`cargo +nightly fmt`** on **`pp-rs`** and any **`lp2025`** files touched.
- Run **`cargo clippy`** on **`lps-naga`** (and **`lps-wasm`** if touched) with *
  *`-D warnings`** per workspace rules.

```bash
cd lp2025
cargo check -p lps-naga
cargo check -p lps-naga --target riscv32imac-unknown-none-elf
cargo check -p lps-wasm --target wasm32-unknown-unknown
cargo test -p lps-naga
```

## Plan cleanup

- Write **`summary.md`** in this directory: fork URL, patch line, validation commands, follow-up (*
  *embedded `lp-engine` GLSL JIT**).
- **`mv docs/plans/2026-03-26-pp-rs-no-std-naga-glsl docs/plans-done/`**

## Commit

Example:

```
chore(deps): patch pp-rs from light-player fork for no_std GLSL

- Add [patch.crates-io] for pp-rs (naga glsl-in) so riscv32imac builds
- Document fork rationale in plans-done
```

Separate commit on **`light-player/pp-rs`**:

```
feat: no_std + alloc for bare-metal naga glsl-in

- hashbrown for HashMap/HashSet; core/alloc for the rest
```

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.
