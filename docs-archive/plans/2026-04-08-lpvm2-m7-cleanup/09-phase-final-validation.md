# Phase 9: Final Validation and Cleanup

## Scope

Final validation that all cleanup is complete. Fix any remaining warnings,
dead code, or TODO comments. Run the full validation matrix.

## Checklist

### [ ] Remove Temporary Code

```bash
# Find any TODO/FIXME/XXX from cleanup
grep -r "TODO.*M7\|FIXME.*M7\|XXX.*cleanup" lp-shader/ lp-core/ lp-fw/ --include="*.rs" || echo "No cleanup TODOs found"

# Find any old API references that should be gone
grep -r "GlslExecutable\|JitModule.*jit(" lp-shader/ lp-core/ --include="*.rs" || echo "No old API references found"
```

### [ ] Check for Dead Code

```bash
# Check for unused imports, dead_code warnings
cargo +nightly clippy -p lpvm-cranelift --lib 2>&1 | grep -E "dead_code|unused_imports" || echo "No dead code warnings"
cargo +nightly clippy -p lpvm-emu --lib 2>&1 | grep -E "dead_code|unused_imports" || echo "No dead code warnings"
cargo +nightly clippy -p lpvm-wasm --lib 2>&1 | grep -E "dead_code|unused_imports" || echo "No dead code warnings"
```

### [ ] Verify No Legacy References

```bash
# Legacy crates should not be referenced
rg "lps-exec|lps-wasm|lps-builtins-wasm" Cargo.toml lp-shader/*/Cargo.toml lp-core/*/Cargo.toml || echo "No legacy crate references"

# wasm_link should be gone
rg "wasm_link" lp-shader/ --glob "*.rs" || echo "No wasm_link references"

# emu_run should be gone (except maybe exports if still needed)
rg "glsl_q32_call_emulated|run_lpir_function_i32" lp-shader/ --glob "*.rs" || echo "No emu_run function references"
```

### [ ] Format Check

```bash
cargo +nightly fmt --check 2>&1 | head -20 || echo "Formatting OK"
```

## Full Validation Matrix

### Host Tests

```bash
# Core LPVM crates
cargo test -p lpvm --lib
cargo test -p lpvm-cranelift --lib
cargo test -p lpvm-emu --lib
cargo test -p lpvm-wasm --lib

# Filetests (all three backends)
cargo test -p lps-filetests --lib
cargo test -p lps-filetests --tests

# Engine and server
cargo test -p lp-engine --lib
cargo test -p lp-engine --tests
cargo test -p lpa-server --lib
cargo test -p lpa-server --tests
```

### Cross-Compilation (Embedded Targets)

```bash
# ESP32 firmware (with cranelift)
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server

# Emulator firmware (RISC-V target)
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

### Firmware Tests

```bash
# These tests run the full firmware in emulator
cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu
```

### Filetest Matrix (CI Parity)

```bash
# Run all filetest targets
./scripts/filetests.sh --target rv32.q32c
./scripts/filetests.sh --target wasm.q32

# If scripts don't exist, run via cargo:
cargo test -p lps-filetests -- --target rv32.q32c
cargo test -p lps-filetests -- --target wasm.q32
```

## Expected Results

| Test                           | Expected            |
| ------------------------------ | ------------------- |
| `cargo test -p lpvm-cranelift` | All pass            |
| `cargo test -p lpvm-emu`       | All pass            |
| `cargo test -p lpvm-wasm`      | All pass            |
| `cargo test -p lp-engine`      | All pass            |
| `cargo test -p lp-server`      | All pass            |
| `cargo test -p lps-filetests`  | All pass            |
| `cargo test -p fw-tests`       | All pass            |
| `fw-esp32 check`               | Clean (no warnings) |
| `fw-emu check`                 | Clean (no warnings) |
| `cargo +nightly fmt --check`   | Clean               |

## Warnings to Fix

If any warnings remain:

1. **Unused imports** - Remove them
2. **Dead code** - Remove or add `#[allow(dead_code)]` with comment explaining why
3. **Documentation warnings** - Add/fix docs
4. **Clippy warnings** - Fix or explicitly allow with comment

## Code Organization Reminders

- No warnings in release builds
- No warnings in cross-compilation targets
- Clean `cargo +nightly fmt --check`

## Plan Completion

After validation passes:

1. Create `summary.md` with what was deleted/changed
2. Move plan to `docs/plans-done/`
3. Commit with message:

```
refactor(cleanup): M7 legacy code removal and consolidation

Delete obsolete crates and APIs from LPVM2 project:
- Remove lps-exec (GlslExecutable trait superseded by LpvmEngine)
- Remove lps-wasm (old WASM emitter, replaced by lpvm-wasm)
- Remove lps-builtins-wasm (old build system)
- Delete jit()/JitModule from lpvm-cranelift (now CraneliftEngine)
- Delete wasm_link.rs from lps-filetests (use lpvm-wasm instead)
- Consolidate emu_run.rs into EmuInstance
- Update lp-engine to use CraneliftEngine trait API
- Update AGENTS.md architecture documentation

All three LPVM backends (cranelift, emu, wasm) now implement
LpvmEngine/LpvmModule/LpvmInstance traits uniformly.
```

## Phase Notes

- This is the final gate - nothing after this but commit and cleanup
- If tests fail, go back to appropriate phase and fix
- Don't commit with warnings
