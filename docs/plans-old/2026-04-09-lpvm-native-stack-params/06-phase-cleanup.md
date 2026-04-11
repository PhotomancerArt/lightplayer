## Phase 6: Cleanup & Validation

## Scope

Final cleanup, validation, and plan completion.

## Cleanup Checklist

### 1. Remove TODO comments and debug code

Search for:
```bash
grep -r "TODO\|FIXME\|XXX\|dbg!\|println!" lp-shader/lpvm-native/src/ --include="*.rs"
```

Remove any temporary debug code added during implementation.

### 2. Fix compiler warnings

```bash
cd /Users/yona/dev/photomancer/feature/lightplayer-native/lp-shader
cargo check -p lpvm-native 2>&1 | grep -E "^warning:|^error:"
```

Address any new warnings introduced by the changes.

### 3. Verify formatting

```bash
cargo +nightly fmt -p lpvm-native -- --check 2>&1
```

Fix any formatting issues.

## Validation

### Unit Tests

```bash
cd /Users/yona/dev/photomancer/feature/lightplayer-native/lp-shader
cargo test -p lpvm-native --lib -- --test-threads=1
```

Expected: All 88+ tests pass.

### File Tests

```bash
# Stack param specific tests
scripts/glsl-filetests.sh function/param-many.glsl --target rv32lp.q32
scripts/glsl-filetests.sh function/param-mixed.glsl --target rv32lp.q32

# Previously failing test that motivated this work
scripts/glsl-filetests.sh function/call-nested.glsl --target rv32lp.q32

# Broader function test coverage
scripts/glsl-filetests.sh function/call-simple.glsl --target rv32lp.q32
scripts/glsl-filetests.sh function/call-multiple.glsl --target rv32lp.q32
```

Expected: All tests pass (0 compile-fail, 0 fail).

### ESP32 Build Check

Per AGENTS.md:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

Expected: Builds without errors.

## Summary of Changes

### Files Modified

1. `lp-shader/lpvm-native/src/regalloc/mod.rs` - Add `incoming_stack_params` to `Allocation`
2. `lp-shader/lpvm-native/src/regalloc/greedy.rs` - Handle `ArgLoc::Stack` in regalloc
3. `lp-shader/lpvm-native/src/abi/frame.rs` - Add `caller_arg_stack_size` to `FrameLayout`
4. `lp-shader/lpvm-native/src/isa/rv32/emit.rs` - Prologue loads and outgoing stores

### Files Created

1. `lp-shader/lps-filetests/filetests/lpvm/native/stack-params-simple.glsl`
2. `lp-shader/lps-filetests/filetests/lpvm/native/stack-params-mixed.glsl`

### Tests Added

- `regalloc::greedy::stack_params_get_registers`
- `abi::frame::frame_with_outgoing_stack_args`
- `emit::prologue_loads_incoming_stack_params`
- `emit::emit_call_with_stack_args`
- File tests: `stack-params-simple.glsl`, `stack-params-mixed.glsl`

## Plan Cleanup

After validation passes, update `summary.md` and move plan to done:

```bash
cp docs/plans/2026-04-09-lpvm-native-stack-params/summary.md docs/plans-done/
mv docs/plans/2026-04-09-lpvm-native-stack-params/ docs/plans-done/
```

## Commit

Commit message:

```
feat(lpvm-native): RISC-V stack parameter support

Enable functions with >8 parameter slots by supporting RV32 ILP32
stack argument passing convention.

- Regalloc: Assign registers for incoming stack params, track offsets
- Frame layout: Reserve caller arg stack area for outgoing calls
- Emit: Prologue loads incoming params, call sequence stores outgoing
- Tests: Unit tests for regalloc/emit, filetests for end-to-end

Verified:
- cargo test -p lpvm-native --lib passes
- function/call-nested.glsl passes on rv32lp.q32
- stack-params-*.glsl tests pass
```
