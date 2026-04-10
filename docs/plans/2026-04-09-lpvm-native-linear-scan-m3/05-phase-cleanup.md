## Phase 5: Cleanup and Validation

### Scope
Clean up temporary code, fix warnings, and ensure all tests pass before commit.

### Cleanup Checklist

#### Code Quality
- [ ] Remove any `todo!()` or `unimplemented!()`
- [ ] Remove debug prints or wrap in `#[cfg(test)]`
- [ ] Ensure all functions have doc comments
- [ ] Check for unused imports

#### Formatting
```bash
cargo +nightly fmt -p lpvm-native
```

#### Warnings
```bash
cargo check -p lpvm-native --lib
# Fix all warnings
```

#### Tests
```bash
# Library tests
cargo test -p lpvm-native --lib

# Filetests
cargo run -p lps-filetests-app -- test lpvm/native/ -t rv32lp.q32

# Compare with greedy (optional - revert to compare)
```

#### Firmware Build
```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

### Final Validation

All must pass:
1. Unit tests: `cargo test -p lpvm-native --lib`
2. Filetests: All native tests pass
3. No compiler warnings
4. Formatted with `cargo +nightly fmt`
5. Firmware builds successfully

### Summary

Add to `summary.md`:
- Lines added/changed
- Performance improvement numbers
- Any known limitations
