# Phase 7: Cleanup, Review, and Validation

## Cleanup & validation

### 1. Remove temporary code

Grep for temporary code, TODOs, debug prints, etc.:

```bash
cd /Users/yona/dev/photomancer/lp2025
git diff --name-only | xargs grep -n "TODO\|FIXME\|XXX\|HACK\|dbg!\|println!"
```

Remove or address any temporary code found.

### 2. Fix warnings and errors

Run full check:

```bash
cd lp-app
cargo check --package fw-emu

cd ../lp-core/lp-client
cargo check --features serial

cd ../../lp-riscv/lp-riscv-emu
cargo check
cargo test --lib
```

Fix all warnings and errors.

### 3. Format code

```bash
cd /Users/yona/dev/photomancer/lp2025
cargo +nightly fmt
```

### 4. Run integration test

```bash
cd lp-app/apps/fw-emu
cargo test --test scene_render
```

Ensure test passes.

### 5. Verify binary builds

```bash
cd lp-app/apps/fw-emu
RUSTFLAGS="-C target-feature=-c" cargo build --target riscv32imac-unknown-none-elf --release
```

Ensure binary builds successfully.

## Plan cleanup

### 1. Add summary

Create `summary.md` in the plan directory with a summary of completed work.

### 2. Move plan files

Once everything is complete and committed, move the plan directory:

```bash
mv docs/plans/2026-01-30-fw-emu-scene-render docs/plans-done/
```

## Commit

Once the plan is complete, and everything compiles and passes tests, commit the changes with a message following the [Conventional Commits](https://www.conventionalcommits.org/) format:

```
feat(fw-emu): implement scene render test with emulator

- Add time mode support to emulator (real-time and simulated)
- Create binary building helper utility for tests
- Implement fw-emu syscall wrappers (serial, time, output)
- Implement fw-emu server loop and main entry point
- Create SerialClientTransport to bridge async client to emulator
- Add integration test that loads scene and renders frames
```
