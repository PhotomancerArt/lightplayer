# Phase 3: Automated matrix (host + embedded crates)

## Scope of phase

Run the **rest of the automated checks** that catch regressions before hardware:
host crates on the critical path and any `no_std` / RISC-V checks already used in
the project.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

1. **Engine / server / compiler** (host tests, typical dev flow):

   ```bash
   cargo test -p lp-engine
   cargo test -p lp-server
   cargo test -p lpir-cranelift
   ```

2. **`lpir-cranelift` embedded profile** (matches VI-A / device-relevant flags):

   ```bash
   cargo test -p lpir-cranelift --no-default-features
   cargo test -p lpir-cranelift --features riscv32-emu
   ```

3. **Clippy** — follow workspace conventions; `justfile` excludes some firmware
   crates from workspace clippy. At minimum:

   ```bash
   cargo clippy -p lp-engine -p lp-server -p lpir-cranelift --all-features -- -D warnings
   ```

4. Fix **warnings** that are not deferred to a later roadmap stage (per project
   rules).

5. No new source files required unless a failure forces a fix.

## Validate

```bash
cargo test -p lp-engine
cargo test -p lp-server
cargo test -p lpir-cranelift
cargo test -p lpir-cranelift --no-default-features
cargo clippy -p lp-engine -p lp-server -p lpir-cranelift --all-features -- -D warnings
```

Re-run Phase 1 commands if any dependency graph change touched `fw-emu`.
