# Phase 4: Filetest Validation

## Scope

Validate straight-line filetests pass under the `rv32fa` target.
Key target: `spill_simple.glsl`

## Implementation

### Update Filetest Runner

The filetest runner likely needs updates to work with the new allocator:

1. **Check pipeline configuration**: `lp-shader/lps-filetests/src/test_run/filetest_lpvm.rs`
   - Ensure `rv32fa` target is wired up
   - May need to use `lpvm_native::compile_function` instead of old path

2. **Update CLI**: `lp-cli/src/commands/shader_rv32fa/pipeline.rs`
   - Ensure it calls the new allocator path
   - Add `--show-alloc` flag for debugging

### Test Validation

Run straight-line filetests:

```bash
# List straight-line tests (no calls, no control flow)
ls docs/filetests/shader/rv32fa/straight/

# Run specific test
cargo test -p lps-filetests -- spill_simple

# Or via CLI
cargo run -p lp-cli -- shader-rv32fa --target rv32fa docs/filetests/shader/rv32fa/spill_simple.glsl
```

### Debug Workflow

When a test fails:

1. **Get allocator output**: Use `--show-alloc` flag
2. **Compare expected**: Check `.txt` expectation file
3. **Update expectation**: If allocator output is correct, update `.txt` file

### Cleanup

- Remove any temporary debug prints
- Fix compiler warnings
- Ensure formatting with `cargo +nightly fmt`

## Validate

```bash
# Build check
cargo check -p lpvm-native
cargo check -p lps-filetests

# Unit tests
cargo test -p lpvm-native

# Filetests
cargo test -p lps-filetests -- --test-threads=1 spill_simple

# Full validation
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p lp-server
```

## Success Criteria

- `spill_simple.glsl` filetest passes
- All straight-line filetests pass
- No regressions in other tests
- All validation commands succeed
