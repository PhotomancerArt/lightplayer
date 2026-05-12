# Phase 7: Cleanup & Validation

## Scope of Phase

Final cleanup, validation, and plan completion. This phase ensures:
- No TODO comments left behind
- No debug prints
- All tests pass
- Code compiles without warnings
- Plan is properly documented

## Code Organization Reminders

- Grep for "TODO", "FIXME", "XXX", "hack", "temp" in modified files
- Remove any debug println! statements
- Ensure all unused functions/imports are removed or marked with `#[allow(dead_code)]`
- Run formatter on all changed files

## Cleanup Checklist

### 1. Grep for temporary code

```bash
cd /Users/yona/dev/photomancer/feature/lightplayer-native
grep -r "TODO\|FIXME\|XXX\|hack\|temp" lp-shader/lpvm-native/src --include="*.rs" | grep -v "^Binary"
```

Review each match and either:
- Fix the issue and remove the comment
- Leave the comment if it's a legitimate future work note (add context)

### 2. Remove debug prints

```bash
grep -r "println!\|eprintln!\|dbg!" lp-shader/lpvm-native/src --include="*.rs"
```

Remove any debug prints that were added during development.

### 3. Check for unused code

```bash
cargo clippy -p lpvm-native -- -W clippy::unused
```

Fix or suppress any legitimate unused code warnings.

### 4. Format code

```bash
cargo +nightly fmt -p lpvm-native
```

### 5. Run full test suite

```bash
# Unit tests
cargo test -p lpvm-native --lib

# Integration tests (if any)
cargo test -p lpvm-native

# Filetests
cd lp-shader/lps-filetests
cargo test --test filetest_runner -- --backend rv32lp.q32 function/

# Firmware emulator tests
cargo test -p fw-tests --test scene_render_emu

# ESP32 build validation
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server

# Host validation
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```

### 6. Verify exports

Ensure `ModuleAbi` is properly exported from `lpvm-native`:

```rust
// In lib.rs or appropriate module file
pub use abi::ModuleAbi;
```

### 7. Documentation

Ensure all public items have appropriate documentation:
- `ModuleAbi` struct and methods
- Updated `VInst::Call` variant
- `FrameLayout` new fields
- Any new error variants

## Validation Commands

Final validation should include:

```bash
# 1. Clean build
cargo clean -p lpvm-native
cargo build -p lpvm-native

# 2. Tests
cargo test -p lpvm-native

# 3. Filetests
cd lp-shader/lps-filetests && cargo test --test filetest_runner -- --backend rv32lp.q32

# 4. ESP32 target (critical for embedded JIT requirement)
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server

# 5. No_std check
cargo check -p lpvm-native --target riscv32imac-unknown-none-elf

# 6. Formatting
cargo +nightly fmt -p lpvm-native -- --check
```

## Plan Completion

Once validation passes:

1. Create `summary.md` with:
   - What was implemented
   - Files changed
   - Test coverage
   - Any known issues or follow-up work

2. Move plan to `docs/plans-done/`

3. Ready for final review
