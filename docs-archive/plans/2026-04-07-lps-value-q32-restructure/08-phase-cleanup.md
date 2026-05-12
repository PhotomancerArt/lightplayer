# Phase 8: Cleanup and Validation

## Scope

Full cleanup, fix all warnings, run full test suite.

## Steps

1. Fix any remaining compilation errors
2. Remove temporary TODO comments
3. Fix all warnings (unused imports, dead_code, etc.)
4. Run formatter
5. Run full test suite

## Validation

```bash
# Check all affected crates
cargo check -p lps-shared -p lpvm -p lpvm-cranelift -p lpvm-emu -p lps-filetests

# Run tests
cargo test -p lps-shared
cargo test -p lpvm
cargo test -p lpvm-cranelift --lib
cargo test -p lpvm-emu --lib

# Check no_std targets
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

## Notes

- Record any design decisions in notes.md
- Summarize changes in summary.md
