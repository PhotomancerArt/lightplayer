# Milestone 4: Cleanup

## Goal

Remove the `lpvm-native` (cranelift-based) crate entirely. Rename
`lpvm-native` to `lpvm-native`. Clean up all references. The `rv32fa`
filetest target name is kept as-is.

## Suggested Plan Name

`fastalloc3-m4`

## Scope

### In scope

- **Delete `lpvm-native` crate**: remove `lp-shader/lpvm-native/` directory and
  its `Cargo.toml` entry
- **Rename `lpvm-native` to `lpvm-native`**: update directory name,
  `Cargo.toml` package name, all `Cargo.toml` dependency references across the
  workspace
- **Remove `rv32::alloc`**: delete the old straight-line allocator that was
  scaffolding
- **Remove `shader-rv32` CLI command**: the old command that used cranelift +
  linear scan. Migrate any useful flags to `shader-rv32fa`.
- **Remove `rv32lp` filetest target**: if it depended on the old crate
- **Update `rv32` filetest target**: point at the new crate if cranelift path is
  still useful, or remove
- **Clean up dead code**: unused modules, cfg-gated code that no longer applies,
  stale imports
- **Update workspace `Cargo.toml`**: remove old crate, update paths
- **Update `AGENTS.md`**: reflect new crate structure
- **Final validation**: all filetests pass, firmware builds pass (`fw-esp32`,
  `fw-emu`), CLI works

### Out of scope

- New allocator features or optimizations
- Pluggable emitter for direct bytecode generation (future roadmap)

## Key Decisions

- The `rv32fa` filetest target name stays — no need to rename it since it
  accurately describes the backend.
- The cranelift filetests (`rv32` target) are removed or retargeted. If there's
  value in keeping cranelift as a reference, it can stay behind a feature gate,
  but the default expectation is full removal.

## Deliverables

- `lpvm-native` directory deleted
- `lpvm-native` renamed to `lpvm-native`
- All workspace `Cargo.toml` references updated
- `shader-rv32` CLI command removed (or merged into `shader-rv32fa`)
- Dead code removed
- All tests passing
- Firmware builds passing

## Dependencies

- M3 (control flow): all filetests pass under `rv32fa` — the new allocator is
  functionally complete

## Estimated Scope

Mostly mechanical renaming and deletion. ~100-200 lines of actual code changes,
but touches many `Cargo.toml` files and import paths across the workspace.

## Validation Commands

```bash
# All filetests
cargo test -p lps-filetests

# Unit tests
cargo test -p lpvm-native --lib

# Firmware builds
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu

# Host builds
cargo check -p lp-server
cargo test -p lp-server --no-run

# CLI
cargo run -p lp-cli -- shader-rv32fa --help
```
