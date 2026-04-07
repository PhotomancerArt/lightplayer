# Milestone 7: Cleanup

## Goal

Delete obsolete code, remove dead traits and crates, verify everything builds
and passes across all targets.

## Suggested plan name

`lpvm2-m7`

## Scope

### In scope

- Delete `GlslExecutable` trait and `lps-exec` crate (if still exists)
- Delete old `JitModule` / `jit()` API from `lpvm-cranelift` (if no longer
  used after M6)
- Remove any remaining `emu_run.rs` remnants from `lpvm-cranelift`
- Remove unused feature flags
- Audit and remove dead dependencies
- Update documentation (AGENTS.md, crate-level docs)
- Run full validation suite:
  - `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server`
  - `cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu`
  - `cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu`
  - `cargo check -p lp-server`
  - `cargo test -p lps-filetests`
  - `cargo +nightly fmt --check`
- Verify no warnings in affected crates

### Out of scope

- New features
- fw-wasm implementation
- Performance optimization

## Key Decisions

1. **Old API removal is gated on M6 completion**: Only delete `JitModule` /
   `jit()` / old `GlslExecutable` paths once all consumers have migrated.

2. **AGENTS.md update**: The validation commands and architecture diagram
   should reflect the new LPVM-based architecture.

## Deliverables

- Removed obsolete code and crates
- Updated documentation
- Clean build across all targets with no warnings
- All tests passing

## Dependencies

- Milestone 6 (engine migration) — all consumers must be migrated before
  deleting old APIs

## Estimated scope

~200–400 lines deleted, ~100 lines of documentation updates. Small milestone
focused on hygiene.
