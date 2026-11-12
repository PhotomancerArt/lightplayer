# Phase 7: Cleanup and Validation

## Scope

Final validation of the abi2 module, documentation review, and preparation for the next plan (wiring abi2 into regalloc and emission).

## Cleanup Tasks

### 1. Grep for TODOs and temp code

```bash
grep -r "TODO\|FIXME\|XXX\|unimplemented" lp-shader/lpvm-native/src/abi/
```

Any `unimplemented!()` for Float register class should remain - that's expected for RV32F future work.

### 2. Documentation review

Verify all public types have doc comments:
- `PReg`, `RegClass`, `PregSet` - purpose and usage clear
- `ArgLoc`, `ReturnMethod` - examples of when each variant occurs
- `FuncAbi` - regalloc vs emission interface clearly distinguished
- `FrameLayout` - layout diagram accurate

### 3. Unused code check

```bash
cargo clippy -p lpvm-native -- -Wunused 2>&1 | grep abi
```

No warnings about unused code in abi2 module (except possibly Float branches).

### 4. Test coverage

Verify each file has comprehensive tests:
- `regset.rs`: set operations, iteration, int vs float distinction
- `rv32/abi2.rs`: all registers defined, sets correct
- `classify.rs`: all return types (void, direct, sret), param reg vs stack
- `func_abi.rs`: allocatable sets, precolors, sret exclusion
- `frame.rs`: layout computation, offsets, alignment

### 5. No_std compatibility

```bash
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf
```

abi2 module should compile for no_std target (no std dependencies).

## Validation Commands

```bash
# Full test suite for abi
cargo test -p lpvm-native abi -- --nocapture

# Check compiles on host
cargo check -p lpvm-native

# Check compiles for embedded
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf

# Clippy warnings
cargo clippy -p lpvm-native -- -D warnings 2>&1 | grep -i "abi2\|error" | head -20

# Format check
cargo +nightly fmt -p lpvm-native -- --check
```

## Transition Readiness Checklist

Before starting the next plan (wiring abi2 into backend):

- [ ] All abi2 tests pass
- [ ] No compilation errors on host or embedded targets
- [ ] No clippy warnings in abi2 code
- [ ] Documentation complete for all public APIs
- [ ] Code formatted with nightly rustfmt
- [ ] Existing abi.rs still functional (no regressions)
- [ ] abi2 module can be imported but is not yet used

## Next Plan Preview

The next plan will wire abi2 into the backend:
1. Update regalloc to accept `PregSet` and `FuncAbi`
2. Update emission to use `FrameLayout` offsets
3. Update prologue/epilogue generation with sret preservation
4. Switch filetests to abi2 path
5. Delete old abi.rs, rename abi2/ → abi/

But that's a separate plan. This one ends here with a fully tested, correct abi2 module ready for integration.
