## Scope of Phase

Validate the sret implementation with filetests and fix any issues.

## Filetests to Run

**Primary test (the one that was failing):**

```bash
scripts/filetests.sh --target rv32lp.q32 scalar/spill_pressure.glsl:15
```

**Related tests:**

```bash
scripts/filetests.sh --target rv32lp.q32 scalar/spill_simple.glsl
scripts/filetests.sh --target rv32lp.q32 vec/vec4_*.glsl
scripts/filetests.sh --target rv32lp.q32 mat/mat4_*.glsl
```

**Full suite to verify no regressions:**

```bash
scripts/filetests.sh --target rv32lp.q32 2>&1 | tail -20
```

## Expected Improvements

| Test                | Before                   | After            |
| ------------------- | ------------------------ | ---------------- |
| spill_pressure.glsl | FAIL: TooManyReturns(16) | PASS             |
| spill_simple.glsl   | PASS                     | PASS (no change) |
| mat4 returns        | FAIL/unsupported         | PASS             |

## Debug Commands

If tests fail:

```bash
# Disassemble generated code
cargo run -p lp-cli -- disasm-rv32 <file.glsl>

# Single test with debug
cargo test -p lps-filetests test_name -- --nocapture
```

## Validate

```bash
# All tests
cargo test -p lpvm-native --lib
cargo test -p lps-filetests

# RV32 native target
scripts/filetests.sh --target rv32lp.q32 scalar/spill_pressure.glsl:15

# Compare with Cranelift JIT (should have same results)
scripts/filetests.sh --target jit.q32 scalar/spill_pressure.glsl:15
```

## Success Criteria

- [ ] `spill_pressure.glsl` passes on `rv32lp.q32` target
- [ ] Numeric results match `jit.q32` (within float precision)
- [ ] No regressions in existing tests
- [ ] All unit tests pass
