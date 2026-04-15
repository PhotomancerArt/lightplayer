# Phase 4: Integration & Validation

## Scope

Ensure all existing tests pass, plus new advanced tests. Clean up and validate.

## Implementation

### 1. Run full test suite

```bash
# All unit tests
cargo test -p lpvm-native

# All filetests (including new ones)
TEST_FILE=spill_simple cargo test -p lps-filetests -- --ignored
TEST_FILE=spill_pressure_3regs cargo test -p lps-filetests -- --ignored
TEST_FILE=param_eviction cargo test -p lps-filetests -- --ignored
TEST_FILE=uvec2 cargo test -p lps-filetests -- --ignored
```

### 2. Fix any regressions

If existing M2 tests fail:
1. Identify the issue (pool size default? entry move logic?)
2. Fix with minimal changes
3. Re-run tests

### 3. Add --show-alloc flag to CLI (optional)

In `lp-cli/src/commands/shader_rv32fa/pipeline.rs`:

```rust
pub struct Verbosity {
    pub vinst: bool,
    pub alloc: bool,  // NEW: show allocation output
    // ...
}
```

When `alloc: true`, render and print `AllocOutput`.

### 4. Format and cleanup

```bash
cargo +nightly fmt --all
cargo fix --lib -p lpvm-native
```

Fix any warnings introduced.

### 5. Documentation update

Update `docs/plans/2026-04-12-fastalloc4-m3/summary.md` with completed work.

## Validation Commands

```bash
# Build checks
cargo check -p lpvm-native
cargo check -p lps-filetests

# ESP32 firmware (must still compile)
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server

# Unit tests
cargo test -p lpvm-native fa_alloc::

# Filetests (all must pass)
TEST_FILE=spill_simple cargo test -p lps-filetests -- --ignored
TEST_FILE=spill_pressure_3regs cargo test -p lps-filetests -- --ignored
TEST_FILE=param_eviction cargo test -p lps-filetests -- --ignored

# All uvec2 tests (existing straight-line)
TEST_FILE=uvec2 cargo test -p lps-filetests -- --ignored
```

## Success Criteria

- All 23 existing M2 filetests pass
- New filetests `spill_pressure_3regs.glsl` and `param_eviction.glsl` pass
- Unit tests for builder pattern pass
- No compiler warnings
- All validation commands succeed
