# Phase 5: Cleanup & Validation

## Scope

Remove old `shader-rv32fa` and `shader-rv32` commands entirely. Clean up any temporary code, fix warnings, and validate everything works.

## Implementation Details

### 1. Delete Old Command Files

Remove these directories entirely:
- `lp-cli/src/commands/shader_rv32/`
- `lp-cli/src/commands/shader_rv32fa/`

### 2. Update `lp-cli/Cargo.toml`

Remove any feature flags or dependencies that were only for the old commands (if any).

### 3. Clean Up Warnings

Check for and fix:
- Unused imports in modified files
- Dead code warnings from removed functionality
- Missing documentation on new public items

```bash
cargo clippy -p lp-cli -- -D warnings
cargo clippy -p lpvm -- -D warnings
cargo clippy -p lpvm-native -- -D warnings
cargo clippy -p lpvm-native -- -D warnings
cargo clippy -p lpvm-emu -- -D warnings
```

### 4. Run Format

```bash
cargo +nightly fmt -p lp-cli -p lpvm -p lpvm-native -p lpvm-native -p lpvm-emu
```

### 5. Full Test Suite

```bash
# Core library tests
cargo test -p lpvm --lib
cargo test -p lpvm-native --lib
cargo test -p lpvm-native --lib
cargo test -p lpvm-emu --lib

# CLI test (compilation)
cargo build -p lp-cli

# Filetests (ensure we didn't break them)
cargo test -p lps-filetests --test filetests -- --ignored 2>&1 | head -100

# Integration test with new command
lp-cli shader-debug -t rv32fa lp-shader/lps-filetests/filetests/debug/rainbow-noctrl-min.glsl
lp-cli shader-debug -t rv32 lp-shader/lps-filetests/filetests/debug/rainbow-noctrl-min.glsl
```

### 6. Grep for TODOs

```bash
grep -r "TODO.*debug" lp-shader/ lp-cli/src/
grep -r "FIXME" lp-shader/ lp-cli/src/
grep -r "unimplemented!" lp-shader/ lp-cli/src/
```

### 7. Verify Filetest Detail Mode Still Works

The filetest detail mode should now use `ModuleDebugInfo`. Ensure it still prints useful information:

```bash
scripts/glsl-filetests.sh --target rv32fa.q32 lpvm/native/perf/caller-save-pressure.glsl
# Check that debug output is still visible in detail mode
```

## Plan Cleanup

After validation, create `summary.md`:

```markdown
# Debug Unification Summary

Completed work:

1. Created `ModuleDebugInfo` and `FunctionDebugInfo` types in `lpvm`
2. Refactored FA backend to populate structured debug info with interleaved format
3. Updated Cranelift backends to populate disassembly sections
4. Created new `shader-debug` CLI command with unified output
5. Removed old `shader-rv32fa` and `shader-rv32` commands

Result:
- Single command `lp-cli shader-debug -t <backend> <file.glsl>`
- Always shows all available sections clearly labeled
- Copy-pasteable help text for discoverability
- Consistent format across all backends
```

Move plan to done:
```bash
mkdir -p docs/plans-done
mv docs/plans/2026-04-14-debug-unification docs/plans-done/
```

## Commit

Commit message:
```
feat(debug): unify compiler debug output with shader-debug command

- Add ModuleDebugInfo and FunctionDebugInfo types to lpvm
- Refactor FA backend to use structured debug info with interleaved format
- Update Cranelift backends to populate disassembly sections
- Create new shader-debug CLI command with clear section-based output
- Remove shader-rv32fa and shader-rv32 commands
- Add copy-pasteable help text for discoverability

The new shader-debug command shows all debug sections for any backend:
  lp-cli shader-debug -t rv32fa file.glsl
  lp-cli shader-debug -t rv32 file.glsl --fn test_foo

Available sections vary by backend:
- rv32fa: interleaved, disasm, vinst, liveness, region
- rv32/rv32lp: disasm only
- jit/wasm: (not available)
```

## Final Validation

```bash
# Build everything
cargo build -p lp-cli

# Test the new command
lp-cli shader-debug --help

# Test with different backends
lp-cli shader-debug -t rv32fa lp-shader/lps-filetests/filetests/debug/rainbow-noctrl.glsl --fn paletteWarm 2>&1 | head -50
lp-cli shader-debug -t rv32 lp-shader/lps-filetests/filetests/debug/rainbow-noctrl.glsl --fn paletteWarm 2>&1 | head -50

# Run full test suite
cargo test -p lpvm-native --lib
cargo test -p lpvm-native --lib
scripts/glsl-filetests.sh --target rv32fa.q32,rv32.q32 lpvm/native/perf/
```
