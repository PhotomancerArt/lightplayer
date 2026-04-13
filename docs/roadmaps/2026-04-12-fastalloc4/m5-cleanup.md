# Milestone 5: Cleanup

## Goal

Remove the `lpvm-native` (cranelift-based) crate entirely. Rename
`lpvm-native-fa` to `lpvm-native`. Clean up all references. Final validation.

## Suggested Plan Name

`fastalloc4-m5`

## Scope

### In scope

- **Delete `lpvm-native` crate**: remove `lp-shader/lpvm-native/` and its
  `Cargo.toml` entry
- **Rename `lpvm-native-fa` to `lpvm-native`**: update directory, package
  name, all `Cargo.toml` dependency references across workspace
- **Remove `rv32::alloc`** if any remnants exist
- **Remove `shader-rv32` CLI command**: migrate useful flags to `shader-rv32fa`
  (or rename `shader-rv32fa` to `shader-rv32`)
- **Update filetest targets**: remove or retarget `rv32lp` if it depended on
  old crate
- **Clean up dead code**: unused modules, stale imports, orphaned cfg gates
- **Update workspace `Cargo.toml`**: remove old crate, update paths
- **Update `AGENTS.md`**: reflect new crate structure
- **Final validation**: all filetests, firmware builds, host builds

### Out of scope

- New allocator features or optimizations
- Pluggable emitter for direct bytecode generation (future work)

## Key Decisions

- The `rv32fa` filetest target name may be kept or renamed to `rv32` since
  it becomes the only native backend. Decide during implementation.
- The cranelift crate is fully removed — no feature-gated preservation.

## Deliverables

- `lpvm-native` directory deleted
- `lpvm-native-fa` renamed to `lpvm-native`
- All workspace references updated
- CLI commands cleaned up
- All tests passing
- Firmware builds passing

## Dependencies

- M4 (control flow): all filetests pass under `rv32fa`

## Estimated Scope

Mostly mechanical renaming and deletion. ~100-200 lines of actual code changes,
touches many `Cargo.toml` files and import paths.

## Validation Commands

```bash
# All filetests
cargo test -p lps-filetests

# Unit tests
cargo test -p lpvm-native --lib

# Firmware builds
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
  --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf \
  --profile release-emu

# Host builds
cargo check -p lp-server
cargo test -p lp-server --no-run

# CLI
cargo run -p lp-cli -- shader-rv32fa --help
```
