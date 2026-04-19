# Phase 5: Cleanup & Validation

## Scope

Final cleanup, validation, and commit preparation.

## Cleanup Checklist

### Code Quality
- [ ] Remove any `TODO(M2)` comments that were completed
- [ ] Remove temporary debug prints (`dbg!`, `println!`)
- [ ] Remove dead code (unused imports, unused functions)
- [ ] Check for `unwrap()`/`expect()` that should be proper errors
- [ ] Ensure all `AllocError` variants are used appropriately

### Formatting
```bash
cargo +nightly fmt -p lpvm-native
cargo +nightly fmt -p lps-filetests
```

### Warnings
```bash
cargo check -p lpvm-native 2>&1 | grep -i warning
```
Fix all warnings or explicitly allow with justification.

### Tests
```bash
# Unit tests
cargo test -p lpvm-native

# Filetests
cargo test -p lps-filetests -- --test-threads=1

# Emulator tests (if applicable)
cargo test -p fw-tests --test scene_render_emu 2>&1 | head -20
```

### Validation Commands
```bash
# ESP32 build
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server

# Emulator build
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu

# Host build
cargo check -p lp-server
cargo test -p lp-server --no-run
```

## Summary Document

Create `docs/plans/2026-04-12-fastalloc4-m2/summary.md`:

```markdown
# M2: Straight-Line Allocator - Complete

## Summary

Implemented backward walk allocator for Linear regions with edit-list architecture.

## Files Added
- fa_alloc/walk.rs - backward walk algorithm
- fa_alloc/render.rs - human-readable output formatting

## Files Modified
- fa_alloc/mod.rs - wired up walk, added snapshot tests
- lps-filetests/... - updated for new allocator

## Test Coverage
- 6+ snapshot tests for allocator cases
- spill_simple.glsl filetest passes
- All 127+ unit tests pass

## Architecture
Following regalloc2 pattern:
- Per-operand allocations in flat table
- Edit list for spills/reloads/moves
- Forward emission applies edits
```

## Commit

```bash
git add -A
git commit -m "feat(fa_alloc): M2 straight-line allocator with edit-list architecture

Implement backward walk allocator for Linear regions:
- walk.rs: backward walk with LRU eviction and spill handling
- render.rs: human-readable snapshot test output
- Snapshot tests for simple, binary, spill, reuse, dead value cases
- spill_simple.glsl filetest passes

Following regalloc2 pattern with per-operand allocations
and edit list for spill/reload/move operations."
```

## Done

M2 complete. Ready for M3 (calls + sret).
