# Phase 6: Testing and Cleanup

## Scope

Validate the implementation works correctly and clean up any temporary code.

## Testing Plan

### 1. Host Filetests (Regression Check)

Existing ELF path tests should still pass:

```bash
# Run existing filetests
cargo test -p lp-shader/lps-filetests --test rv32lp_smoke

# Check that ELF emission still works
cargo check -p lpvm-native --features emu
```

### 2. Firmware Build Validation

```bash
# ESP32 firmware builds successfully
cargo build -p fw-esp32 \
    --target riscv32imac-unknown-none-elf \
    --profile release-esp32 \
    --features esp32c6,server

# Emulator firmware builds successfully
cargo build -p fw-emu \
    --target riscv32imac-unknown-none-elf \
    --profile release-emu
```

### 3. Binary Size Check

Compare binary sizes before/after:

```bash
# Before (baseline with ELF)
# TODO: measure baseline

# After (with JIT)
cargo build -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
ls -la target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32

# Expected: ~55KB smaller than ELF version
```

### 4. Runtime Tests (Manual for now)

Create a simple test shader and verify it compiles + executes:

```glsl
// simple_add.glsl
float simple_add(float a, float b) {
    return a + b;
}
```

Test in fw-emu:
1. Load shader source
2. Compile with JIT engine
3. Call `simple_add(1.0, 2.0)`
4. Verify result is 3.0

### 5. Code Cleanup Checklist

Grep for and fix:

```bash
# Find TODO comments
grep -r "TODO" lp-shader/lpvm-native/src/rt_jit/

# Find unwrap() calls that should be proper errors
grep -r "unwrap()" lp-shader/lpvm-native/src/rt_jit/

# Find unimplemented!() markers
grep -r "unimplemented" lp-shader/lpvm-native/src/rt_jit/

# Check for debug prints
grep -r "println!" lp-shader/lpvm-native/src/rt_jit/
```

## Fix Any Issues Found

Common issues to address:

1. **Incomplete builtin_address match** - Add remaining builtins to the match statement
2. **Multi-arg call handling** - Implement proper stack setup for many arguments
3. **Sret return handling** - Detect sret from metadata and load from buffer
4. **Error handling** - Replace unwrap with proper error types

## Documentation Updates

Update any relevant documentation:

1. Add module-level doc comments explaining JIT usage
2. Update README if there's a JIT section
3. Document the BuiltinTable population pattern

## Final Validation Commands

```bash
# 1. Host checks pass
cargo check -p lpvm-native
cargo check -p lpvm-native --features emu

# 2. RISC-V checks pass
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf

# 3. Firmware builds pass
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf

# 4. Filetests pass
cargo test -p lp-shader/lps-filetests --test rv32lp_smoke -- --test-threads=1

# 5. Formatting
cargo +nightly fmt --all -- --check

# 6. Clippy (if enabled)
cargo clippy -p lpvm-native --target riscv32imac-unknown-none-elf -- -D warnings 2>&1 | head -50
```

## Create Summary

Once complete, create `summary.md`:

```markdown
# M4: rt_jit - JIT Buffer Compilation - Summary

## Completed Work

- Created `rt_jit` module for RISC-V targets
- Implemented `BuiltinTable` for symbol resolution
- Implemented `JitBuffer` for executable memory
- Implemented `JitEmitContext` following cranelift pattern
- Implemented `NativeJitEngine/Module/Instance` (Lpvm trait implementations)
- Integrated into fw-esp32 and fw-emu firmwares

## Results

- Binary size: Saved ~55KB flash (removed object crate + ELF linking)
- Performance: Direct function calls (no emulation overhead)
- Memory: JIT buffers allocated on heap as needed

## Validation

- Host filetests: PASS (ELF path unchanged)
- Firmware builds: PASS
- Runtime tests: PASS (manual verification)

## Follow-up Work

- Create EmuJit for more direct testing (if needed)
- Expand builtin coverage in builtin_address()
- Optimize multi-argument calls
```

## Move Plan to Done

```bash
# Move plan files to done directory
mkdir -p docs/plans-done/2026-04-09-lpvm-native-rt-jit-m4
cp docs/plans/2026-04-09-lpvm-native-rt-jit-m4/* docs/plans-done/2026-04-09-lpvm-native-rt-jit-m4/
rm -rf docs/plans/2026-04-09-lpvm-native-rt-jit-m4
```

## Commit

```bash
git add -A
git commit -m "feat(lpvm-native): JIT buffer compilation for RISC-V

Implement direct JIT compilation for fw-emu and fw-esp32:

- Add rt_jit module (target_arch = riscv32 gated)
- BuiltinTable for symbol resolution (populated at startup)
- JitBuffer for executable memory management
- JitEmitContext following cranelift finalize pattern
- NativeJitEngine/Module/Instance trait implementations
- Integrate into firmware builds

Results:
- ~55KB flash savings (removed object crate dependency)
- Direct builtin calls (no ELF linking overhead)
- Works on no_std + alloc

Validation:
- Filetests pass (ELF path unchanged)
- Firmware builds succeed
- Manual runtime tests pass"
```

## Acceptance Criteria

- [ ] Host filetests still pass
- [ ] fw-emu builds and runs simple shader
- [ ] fw-esp32 builds
- [ ] Binary size reduced vs ELF path
- [ ] No TODOs remaining in rt_jit module
- [ ] Summary.md written
- [ ] Plan moved to docs/plans-done/
- [ ] Committed with conventional commit message
