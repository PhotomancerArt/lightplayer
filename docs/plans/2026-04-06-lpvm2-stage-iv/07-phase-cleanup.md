## Phase 7: Cleanup and Validation

Final phase: fix all warnings, ensure formatting, comprehensive testing.

### Cleanup Checklist

1. **Remove TODO comments** - grep for `TODO`, `FIXME`, `unimplemented!` in new code
2. **Fix unused imports** - run `cargo check` and clean warnings
3. **Fix dead code** - remove or allow with reason
4. **Documentation** - ensure public APIs have doc comments
5. **Formatting** - run `cargo +nightly fmt`

### Grep for temporary code

```bash
grep -r "TODO\|FIXME\|XXX\|unimplemented!" lp-shader/lpvm-emu/src/
grep -r "TODO\|FIXME\|XXX\|unimplemented!" lp-riscv/lp-riscv-emu/src/emu/memory.rs
```

### Validation Commands

```bash
# Check all affected crates compile
cargo check -p lp-riscv-emu
cargo check -p lp-riscv-emu --no-default-features
cargo check -p lpvm-emu
cargo check -p lpvm-emu --no-default-features
cargo check -p lpvm-cranelift
cargo check -p lpvm-cranelift --no-default-features
cargo check -p lps-filetests

# Per repo rules: validate embedded build
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu

# Run tests
cargo test -p lp-riscv-emu
cargo test -p lpvm-emu
cargo test -p lpvm-cranelift

# Format
cargo +nightly fmt -p lp-riscv-emu -p lpvm-emu -p lpvm-cranelift
```

### Summary of Changes

After this plan completes:

1. `lp-riscv-emu`: Has shared memory region at 0x40000000, backward compatible
2. `lpvm-emu`: New crate with `EmuEngine`, `EmuModule`, `EmuInstance` implementing LPVM traits
3. `lpvm-cranelift`: Removed `lp-riscv-emu` dependency and `riscv32-emu` feature, `emu_run.rs` moved
4. `lps-filetests`: Temporarily updated to use `lpvm-emu`

### Ready for M5

The filetests can now be migrated to use `EmuEngine` directly in M5.
