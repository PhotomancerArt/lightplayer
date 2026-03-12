# Phase 5: Cleanup & Validation

## Scope

Final cleanup, warning fixes, and validation before commit.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Cleanup & Validation

### 1. Grep for temporary code

```bash
git diff --name-only | xargs grep -n "TODO\|FIXME\|HACK\|TEMP\|debug_assert\|dbg!\|println!"
```

Remove any temporary debug prints, TODOs that are resolved, or hack workarounds.

### 2. Fix warnings and formatting

```bash
# Format
cargo +nightly fmt

# Clippy for all modified crates
cargo clippy -p lp-riscv-emu-shared
cargo clippy -p lp-riscv-emu-guest --target riscv32imac-unknown-none-elf
cargo clippy -p lp-riscv-emu-guest --target riscv32imac-unknown-none-elf --features alloc-trace
cargo clippy -p lp-riscv-elf --features std
cargo clippy -p lp-riscv-emu --features std
cargo clippy -p lp-cli
cargo clippy -p fw-tests
```

### 3. Run all tests

```bash
# Unit tests
cargo test -p lp-riscv-emu-shared
cargo test -p lp-riscv-emu
cargo test -p lp-riscv-elf

# Integration tests
cargo test -p fw-tests test_alloc_trace
cargo test -p fw-tests test_scene_render_fw_emu

# Full test suite (if time permits)
cargo test
```

### 4. End-to-end validation

```bash
# Run against a real project
just emu-trace <path-to-test-project> 10

# Verify output is reasonable
cat traces/*/meta.json | python3 -m json.tool
wc -l traces/*/heap-trace.jsonl
head -3 traces/*/heap-trace.jsonl
```

## Plan Cleanup

### Summary

Write `docs/plans/2026-03-08-alloc-trace/summary.md` with:
- What was implemented
- File changes made
- How to use it

### Move plan

```bash
mv docs/plans/2026-03-08-alloc-trace docs/plans-done/2026-03-08-alloc-trace
```

## Commit

```
feat(alloc-trace): add allocation tracing for emulator memory debugging

- Add SYSCALL_ALLOC_TRACE syscall for guest→host allocation events
- Implement TrackingAllocator in lp-riscv-emu-guest (feature-gated)
- Implement AllocTracer in lp-riscv-emu (JSON Lines output with backtraces)
- Add build_symbol_list() to lp-riscv-elf for symbol ranges with sizes
- Add lp-cli emu-trace command for end-to-end tracing
- Add just emu-trace recipe for easy invocation
- Add integration test in fw-tests
```
