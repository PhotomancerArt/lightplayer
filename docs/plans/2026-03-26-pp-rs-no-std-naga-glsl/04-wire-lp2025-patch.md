# Phase 4: Wire `lp2025` `[patch.crates-io]`

## Scope of phase

Point the **workspace** at **`light-player/pp-rs`** so every **`pp-rs`** edge (through **naga**) uses the fork.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

1. In **`lp2025/Cargo.toml`**, under **`[patch.crates-io]`**, add **`pp-rs`** (done: **`path = "lp-glsl/pp-rs"`** until **`light-player/pp-rs`** exists). Then prefer:
   ```toml
   pp-rs = { git = "https://github.com/light-player/pp-rs", branch = "main" }
   ```
   (Adjust **branch** if you use **`lightplayer`** / version branches.)

2. Mirror the **local dev** pattern used for **Cranelift**: in **`# Local dev patches`**, a **commented** **`git`** override (see root **`Cargo.toml`** — uncomment **`git`** and drop **`path`** when the GitHub repo is live). Optional sibling clone: `# pp-rs = { path = "../pp-rs" }`.

3. Run **`cargo update -p pp-rs`** (or full refresh) so **`Cargo.lock`** records the **git** source.

4. **Do not** add **`pp-rs`** to **`[workspace.dependencies]`** unless you need a shared version pin; **patch** alone is enough.

## Validate

```bash
cd lp2025
cargo check -p lp-glsl-naga
cargo check -p lp-glsl-naga --target riscv32imac-unknown-none-elf
cargo check -p lp-glsl-wasm --target wasm32-unknown-unknown
cargo test -p lp-glsl-filetests --no-run   # or a lighter subset if full run is heavy
```

## Tests to write

- None beyond existing **`lp-glsl-naga`** / filetests if run.
