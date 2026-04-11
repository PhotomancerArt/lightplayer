## Scope of Phase

Final cleanup: remove TODOs, fix warnings, format code, update documentation.

## Cleanup Checklist

### Code Cleanup
- [ ] Grep for `TODO(sret)`, `FIXME`, `XXX` - remove or file issues
- [ ] Remove any debug print statements
- [ ] Remove commented-out code
- [ ] Check for unused imports
- [ ] Verify all match arms handled (no `_ =>` wildcards without thought)

### Formatting
```bash
cargo +nightly fmt -p lpvm-native
cargo +nightly fmt -p lps-filetests
```

### Warnings
```bash
cargo check -p lpvm-native 2>&1 | grep warning
cargo clippy -p lpvm-native -- -D warnings 2>&1 || true
```

### Lints to Fix
- Unused variables
- Dead code
- Missing docs on public items (if applicable)

### Documentation
- [ ] Update `abi.rs` module docs to describe sret handling
- [ ] Update `emit.rs` docs for sret emission
- [ ] Add doc comments to `AbiInfo` struct

### Commit Preparation
```bash
# Review changes
git diff --stat
git diff lp-shader/lpvm-native/

# Check for accidental changes
git diff --name-only | grep -v lp-shader/lpvm-native | grep -v docs/plans
```

## Validation

```bash
# Full test suite
cargo test -p lpvm-native --lib
cargo test -p lps-filetests
scripts/glsl-filetests.sh --target rv32lp.q32

# ESP32 build (if applicable)
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

## Plan Summary

Add to `docs/plans-done/2026-04-09-lpvm-native-abi-sret/summary.md`:

```markdown
# Summary: lpvm-native Sret ABI Implementation

## Completed Work

Implemented RISC-V RV32 sret (struct-return) calling convention for functions returning >4 scalars.

### Changes
- `abi.rs`: Added `AbiInfo` struct for per-function ABI classification
- `emit.rs`: Thread `LpsFnSig` through emission, handle sret returns (stores to buffer)
- `instance.rs`: Caller-side sret buffer allocation, arg shifting, result readback

### Tests Passing
- `scalar/spill_pressure.glsl:15` - mat4 return via sret
- `scalar/spill_simple.glsl` - still works (direct return)
- All existing unit tests

### ABI Behavior
- Sret threshold: >4 scalars (>16 bytes)
- Buffer pointer passed in a0
- Real args shifted to a1-a7
- Callee stores to a0-relative offsets
- Caller reads return values from buffer
```

## Move to plans-done

```bash
mv docs/plans/2026-04-09-lpvm-native-abi-sret docs/plans-done/
```
