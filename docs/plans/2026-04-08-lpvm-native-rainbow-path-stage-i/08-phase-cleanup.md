## Scope of Phase

Cleanup: remove TODOs, fix warnings, final validation.

## Code Organization Reminders

- Search for TODO comments, resolve or file issues
- Check for unused imports, dead code
- Run rustfmt
- Ensure all test names are descriptive

## Cleanup Checklist

### Remove temporary code

```bash
# Find TODOs
grep -r "TODO" lp-shader/lpvm-native/src/
grep -r "FIXME" lp-shader/lpvm-native/src/
grep -r "XXX" lp-shader/lpvm-native/src/
grep -r "unimplemented" lp-shader/lpvm-native/src/
```

Common patterns to clean up:
- `// TODO: phase X` — should be done, remove comment
- `unimplemented!()` — replace with proper error or implementation
- `println!` debug statements — remove
- `#[allow(dead_code)]` — check if still needed

### Fix warnings

```bash
cargo check -p lpvm-native 2>&1 | grep warning
cargo clippy -p lpvm-native 2>&1 | grep -A2 "warning:"
```

Common warnings to fix:
- Unused imports
- Unused variables (prefix with `_` or remove)
- Dead code
- Missing `#[must_use]` on results

### Format

```bash
cargo +nightly fmt -p lpvm-native
```

## Validation Commands

Final validation must pass:

```bash
# Unit tests
cargo test -p lpvm-native --lib

# All filetests
cargo test -p lps-filetests

# Check builds (host and target)
cargo check -p lpvm-native
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf

# Firmware build
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6

# No warnings
cargo clippy -p lpvm-native -- -D warnings 2>&1 | head -20
```

## Summary Document

Create `summary.md`:

```markdown
# M1 Summary: ABI - sret, Multi-Return, Out-Params

## Completed

- Return classification (Direct vs Sret)
- Stack slot layout for LPIR slots
- Frame layout with s0-relative negative offsets
- Multi-return emission (a0-a3)
- Greedy allocator emergency spill support
- Spill code emission (LoadSpill, StoreSpill)
- Filetest: spill_pressure.glsl

## Key Decisions

- Frame pointer (s0) with negative offsets (QBE-style)
- 16-byte threshold for sret (4 scalars on RV32)
- Emergency spill in greedy allocator as baseline

## Deferred to M2

- LPIR lowering to use multi-return
- Insertion of spill code around vreg uses/defs
- Full rainbow shader support

## Files Modified

- `isa/rv32/abi.rs`
- `isa/rv32/emit.rs`
- `regalloc/greedy.rs`
- `regalloc/mod.rs`
- `vinst.rs`
- `types.rs`
- `error.rs`

## Validation

All tests pass: `cargo test -p lpvm-native`
```

## Sign-off

- [ ] All TODOs resolved or documented
- [ ] No warnings in `cargo clippy`
- [ ] Code formatted with `cargo fmt`
- [ ] All tests pass
- [ ] Summary document written
- [ ] Ready to move to M2
