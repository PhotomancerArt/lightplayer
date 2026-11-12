# Phase 6: Cleanup and Validation

## Scope

- Remove TODOs and debug prints
- Fix warnings
- Verify all tests pass
- Check no-std compilation works
- Format code

## Code Organization Reminders

- Run `cargo fmt` on all changed files
- Check for unused imports or dead code
- Verify no `todo!()` or `unimplemented!()` in final code
- Review `expect()` messages for helpfulness

## Cleanup Tasks

### Remove temporary code

```bash
# Find TODOs introduced in this work
grep -r "TODO(debug)" lp-shader/lpvm-native/src/
grep -r "TODO(M2.1)" lp-shader/lpvm-native/src/
grep -r "println!" lp-shader/lpvm-native/src/isa/rv32/debug/
grep -r "eprintln!" lp-shader/lpvm-native/src/isa/rv32/debug/
```

### Fix warnings

```bash
cargo clippy -p lpvm-native -- -D warnings 2>&1 | head -50
cargo clippy -p lp-cli -- -D warnings 2>&1 | head -50
```

Common issues to watch for:
- Unused imports in `debug/` modules
- Dead code in test modules
- `Option<u32>` that could be more specific types
- Missing documentation on public items

### Check test coverage

```bash
# Run all lpvm-native tests
cargo test -p lpvm-native --lib

# Run specific debug tests
cargo test -p lpvm-native --lib debug
cargo test -p lpvm-native --lib line_table
cargo test -p lpvm-native --lib disasm

# Run lp-cli tests if any
cargo test -p lp-cli
```

### Verify no-std compilation

```bash
# Check embedded target compilation
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf

# Verify debug module is properly gated
# (Should be excluded or work without std)
```

### Formatting

```bash
cargo +nightly fmt -p lpvm-native
cargo +nightly fmt -p lp-cli
```

## Validation Commands

Full validation sequence:

```bash
# 1. Check compilation
cargo check -p lpvm-native
cargo check -p lp-cli

# 2. Check tests
cargo test -p lpvm-native --lib
cargo test -p lp-cli

# 3. Check clippy
cargo clippy -p lpvm-native -- -D warnings
cargo clippy -p lp-cli -- -D warnings

# 4. Check no-std
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf

# 5. Format check
cargo +nightly fmt -p lpvm-native -- --check
cargo +nightly fmt -p lp-cli -- --check

# 6. Integration test - CLI works
cargo run -p lp-cli -- shader-rv32 filetests/scalar/int/op-add.glsl > /tmp/test.s
test -s /tmp/test.s && echo "Output generated successfully"

# 7. Check output is valid assembly
grep -q ".globl" /tmp/test.s && echo "Has .globl directive"
grep -q "LPIR\[" /tmp/test.s && echo "Has LPIR annotations"
```

## Success Criteria

- [ ] All `cargo test -p lpvm-native` tests pass
- [ ] `cargo clippy` reports no warnings
- [ ] `cargo +nightly fmt` produces no changes
- [ ] No-std target compiles: `cargo check --target riscv32imac-unknown-none-elf`
- [ ] CLI command works: `cargo run -p lp-cli -- shader-rv32 <file.glsl>`
- [ ] Output contains function labels and LPIR annotations
- [ ] No TODO or debug print statements in production code

## Commit Message

Once validated, commit with:

```
feat(lpvm-native,lp-cli): M2.1 debug infrastructure

- Add src_op tracking to VInst for LPIR→RV32 correlation
- Add debug line recording to EmitContext
- Implement LineTable for PC→source lookup
- Add annotated disassembly (RV32 + LPIR comments)
- Add lp-cli shader-rv32 command for assembly output

Provides human-readable annotated assembly showing which
LPIR operation generated each instruction. Foundation for
future DWARF support and emulator debugging.

Validation:
- cargo test -p lpvm-native
- cargo run -p lp-cli -- shader-rv32 <file.glsl>
- cargo check --target riscv32imac-unknown-none-elf
```
