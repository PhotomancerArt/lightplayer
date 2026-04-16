# Phase 4: Wire `lp2025` `[patch.crates-io]`

## Scope of phase

Point the **workspace** at **`light-player/pp-rs`** so every **`pp-rs`** edge (through **naga**)
uses the fork.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

1. In **`lp2025/Cargo.toml`**, under **`[patch.crates-io]`** (done):
   ```toml
   pp-rs = { git = "https://github.com/light-player/pp-rs", branch = "main" }
   ```

2. **Local dev:** in **`# Local dev patches`**, uncomment **`#[patch.crates-io]`** + *
   *`pp-rs = { path = "../pp-rs" }`** after cloning **`https://github.com/light-player/pp-rs`** as *
   *`../pp-rs`** (sibling of **`lp2025`**), and comment out the **`git`** patch line above so Cargo
   resolves one source only.

3. Run **`cargo update -p pp-rs`** (or full refresh) so **`Cargo.lock`** records the **git** source.

4. **Do not** add **`pp-rs`** to **`[workspace.dependencies]`** unless you need a shared version
   pin; **patch** alone is enough.

## Validate

```bash
cd lp2025
cargo check -p lps-frontend
cargo check -p lps-frontend --target riscv32imac-unknown-none-elf
cargo check -p lps-wasm --target wasm32-unknown-unknown
cargo test -p lps-filetests --no-run   # or a lighter subset if full run is heavy
```

## Tests to write

- None beyond existing **`lps-frontend`** / filetests if run.
