# Phase 3: Cross-target validation (RISC-V + WASM)

## Scope of phase

Prove **`pp-rs`** + **`lps-frontend`** compile for **bare-metal RISC-V** and that **WASM** / host
paths are unchanged.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

1. From **`lp2025`** root, with **`[patch.crates-io]`** temporarily pointing **`pp-rs`** to *
   *`path = "../pp-rs"`** (or after Phase 4, git):
   ```bash
   rustup target add riscv32imac-unknown-none-elf
   cargo check -p lps-frontend --target riscv32imac-unknown-none-elf
   ```
2. WASM sanity (unchanged **`std`** on target):
   ```bash
   cargo check -p lps-wasm --target wasm32-unknown-unknown
   ```
3. Host:
   ```bash
   cargo test -p lps-frontend
   ```

## Validate

All three commands above succeed with **no new warnings** in **`pp-rs`** / **`lps-frontend`** that
violate **lp2025** lint policy (fix or allow with justification).

## Tests to write

- Optional **GitHub Actions** (on **`light-player/pp-rs`**): matrix **`ubuntu-latest`** with *
  *`cargo check`** + **`riscv32imac-unknown-none-elf`** + **`wasm32-unknown-unknown`** for a minimal
  crate that depends only on **`pp-rs`**, or document that **`lp2025`** CI is the integration gate.
