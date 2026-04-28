# Phase 8: Cleanup, Validation, Plan Closure

## Scope

Final cleanup, comprehensive testing, documentation updates, and plan closure.

## Checklist

### [ ] Code Cleanup

- [ ] Run `cargo +nightly fmt` on all changed files
- [ ] Review all `TODO`, `FIXME`, `XXX` comments — resolve or ticket
- [ ] Remove any debug `println!` or `log::debug!` added during development
- [ ] Check for unused imports, dead code (run `cargo +nightly clippy`)
- [ ] Verify no `panic!` in library code paths (only in constructors/firmware)

### [ ] Remove Deprecated API

- [ ] Confirm `ProjectRuntime::without_graphics()` removed (or converted to test mock)
- [ ] Confirm old `ShaderRuntime::new(node_handle)` removed (must pass graphics)
- [ ] Update any tests using deprecated constructors

### [ ] File Tests

```bash
# Run all GLSL filetests (both rv32 and wasm targets)
./scripts/filetests.sh --target rv32.q32c
./scripts/filetests.sh --target wasm.q32
```

Expected: All pass, no new failures.

### [ ] Firmware Tests

```bash
# Emulator integration tests (full server stack with graphics)
cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu

# Compile check ESP32 with cranelift
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server

# Compile check ESP32 without cranelift (should still compile, fail at runtime for shaders)
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server --no-default-features
```

### [ ] Host Tests

```bash
# Server tests
cargo test -p lp-server --lib

# Engine tests with cranelift
cargo test -p lp-engine --lib --features test-cranelift

# Engine tests without cranelift
cargo test -p lp-engine --lib --no-default-features
```

### [ ] Documentation

- [ ] Add module-level docs to `gfx/mod.rs` explaining the abstraction
- [ ] Document `LpGraphics` and `LpShader` traits with usage examples
- [ ] Update `docs/plans/2026-04-07-lpvm2-m6-migrate-engine/summary.md`
- [ ] Update `docs/roadmaps/2026-04-06-lpvm2/m6-migrate-engine.md` with completion notes

### [ ] Plan Closure

- [ ] Move plan folder to completed state (or archive)
- [ ] Update any links in other docs that reference M6
- [ ] Create follow-up tickets for:
  - M7: Texture cache integration with `LpGraphics`
  - M8: WASM backend (`WasmGraphics`)
  - M9: GPU backend exploration (`GpuGraphics`)

## Validation Commands

```bash
# Full test suite
cargo test -p fw-tests
cargo test -p lp-engine
cargo test -p lp-server

# ESP32 build verification
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server

# Format check
cargo +nightly fmt --check

# Clippy
cargo +nightly clippy -p lp-engine --lib --features test-cranelift -- -D warnings
```

## Sign-off

When all checkboxes are complete and validation passes:

1. Commit with message: `feat(gfx): LpGraphics abstraction for pluggable shader backends`
2. Push to `feature/lpvm` branch
3. M6 milestone complete — ready for M7
