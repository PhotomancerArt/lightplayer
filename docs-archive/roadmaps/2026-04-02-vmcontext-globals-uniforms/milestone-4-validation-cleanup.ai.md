# Milestone IV: Validation and Cleanup

## Goal

Validate the full implementation across all targets, clean up temporary scaffolding, and ensure
embedded builds remain functional.

## Suggested Plan Name

`vmcontext-globals-uniforms-milestone-4`

## Scope

**In scope:**

- Comprehensive filetests for uniforms and globals
- WASM browser integration test
- RISC-V32 embedded build validation (`fw-esp32`, `fw-emu`)
- Documentation updates (crate docs, examples)
- Performance baseline measurement
- Remove any temporary scaffolding from earlier milestones

**Out of scope:**

- New features (fuel, trap handlers)
- Optimization work beyond basic validation

## Key Decisions

1. **Embedded first**: Validate RISC-V32 builds early and often
2. **Test coverage**: Every uniform/global pattern should have a filetest
3. **Documentation**: Update crate-level docs with VMContext examples

## Deliverables

- Passing filetests for all uniform/global patterns
- Passing `fw-esp32` and `fw-emu` builds
- Passing WASM tests
- Updated `lpvm` README with VMContext usage
- Updated `docs/design/uniforms-globals.md` with final implementation notes

## Dependencies

- Milestone III: Mutability, _init(), and Reset

## Estimated Scope

~300 lines: tests (200), docs (100)

## Validation Commands

```bash
# Core tests
cargo test -p lpvm -p lpvm-cranelift -p lps-frontend

# Filetests
cargo test -p lps-filetests

# Embedded builds
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu

# WASM build
cargo check -p lps-wasm
```
