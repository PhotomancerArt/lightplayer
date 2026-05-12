## Phase 8: Cleanup and validation

### Scope

- Grep for `TODO(M3)`, `TODO(rt_emu)`, temporary `dbg!`, stale comments
- Run `cargo +nightly fmt` on all touched files
- Update `CRATES.md` or crate documentation if structure changed significantly

### AGENTS validation

```bash
# Core no_std build (embedded target)
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf

# Emulation feature (host)
cargo test -p lpvm-native --features emu --lib

# Filetests integration
cargo check -p lps-filetests
cargo test -p lps-filetests --lib

# Firmware (if lp-engine or fw-esp32 gain dependency)
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server

# Emulator firmware tests
cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu
```

### Fix all warnings

Ensure no dead code, unused imports, or deprecated feature names remain.

### Plan completion

- Add `summary.md` to plan directory
- Move plan to `docs/plans-done/`
- Commit with Conventional Commits format:
  ```
  feat(lpvm-native): rt_emu runtime with link and emulate support
  
  - Add rt_emu/ module (engine, module, instance)
  - Feature "emu" gates emulation runtime
  - NativeEmuEngine compiles, links with builtins, emulates
  - Add rv32lp backend to filetests
  ```
