## Scope of Phase

Cleanup, validation, and final testing for M2.1.

## Code Organization Reminders

- Grep for TODOs, FIXMEs, and temporary code
- Remove any debug print statements
- Ensure all new code has appropriate tests
- Run full test suite
- Check with clippy if available

## Implementation Details

### Validation Checklist

1. **Code quality checks**:
```bash
# Check for TODOs
grep -r "TODO\|FIXME\|XXX" lp-shader/lpvm-native/src/ --include="*.rs"

# Check for debug prints
grep -r "println!\|eprintln!" lp-shader/lpvm-native/src/ --include="*.rs"
```

2. **Build and test**:
```bash
# Check all targets
cargo check -p lpvm-native
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf

# Run all tests
cargo test -p lpvm-native

# Check formatting (if nightly available)
cargo +nightly fmt -p lpvm-native -- --check
```

3. **Test coverage verification**:
   - [ ] All new VInsts have `src_op()`, `defs()`, `uses()` tests
   - [ ] All new encoders have unit tests
   - [ ] All new lowering paths have tests
   - [ ] All new emission paths have tests
   - [ ] Integration: compile and run a simple shader using new ops

### Integration Test

Create a simple end-to-end test in `rv32lp_smoke.rs` or as a filetest:

```rust
#[test]
fn compile_and_run_div_rem() {
    // Build IR with division and remainder
    // Compile
    // Execute and verify results
}

#[test]
fn compile_and_run_icmp() {
    // Build IR with comparisons
    // Compile
    // Execute and verify results
}

#[test]
fn compile_and_run_select() {
    // Build IR with select
    // Compile
    // Execute and verify results
}
```

### Filetest Support

If filetests exist for new ops, verify they pass:

```bash
cargo test -p lps-filetests --test filetests -- scalar/icmp.glsl
cargo test -p lps-filetests --test filetests -- scalar/select.glsl
```

## Acceptance Criteria

- [ ] All 62+ existing tests still pass
- [ ] New encoder tests pass (8+ new tests)
- [ ] New lowering tests pass (14+ new tests)
- [ ] New emission tests pass (4+ new tests)
- [ ] No TODOs or FIXMEs remaining
- [ ] No compiler warnings
- [ ] No clippy warnings (if running clippy)
- [ ] RISC-V target builds successfully
- [ ] Code formatted with rustfmt

## Summary

Add to `summary.md`:

```markdown
# M2.1 Core Integer Operations - Summary

## Completed

- Added 7 new VInst variants: And32, DivS32, DivU32, RemS32, RemU32, Icmp32, Select32
- Added IcmpCond enum for comparison conditions
- Added RV32 instruction encoders: and, div, divu, rem, remu, slt, sltu, sltiu, xor, xori
- Lowering support for: IdivS/U, IremS/U, Ieq/Ine/IltS/etc (10 comparisons), Select
- Emission support for all new VInsts with proper instruction sequences
- Select implemented as branchless arithmetic (sub + and + add)

## Tests Added

- Encoder tests: 8
- Lowering tests: 14 (4 div/rem, 10 comparisons, 1 select)
- Emission tests: 4

## Lines Changed

~400 lines added across vinst.rs, inst.rs, lower.rs, emit.rs
```

## Next Steps

Move plan to done and start M2.2 Control Flow:

```bash
mv docs/plans/2026-04-09-lpvm-native-m2-1-core-integer docs/plans-done/
```
