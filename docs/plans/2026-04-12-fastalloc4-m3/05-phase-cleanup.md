# Phase 5: Cleanup & Commit

## Scope

Final cleanup, summary, and commit.

## Cleanup Checklist

- [ ] Grep for TODO, FIXME, HACK, XXX - remove or file issues
- [ ] Grep for `println!`, `eprintln!`, `dbg!` - remove debug prints
- [ ] Grep for `unimplemented!()`, `panic!`, `todo!` - ensure they're appropriate
- [ ] Check for dead code warnings
- [ ] Run `cargo +nightly fmt --all`
- [ ] Run `cargo clippy -p lpvm-native-fa` (if available)

## Validation

```bash
# Final validation
cargo check -p lpvm-native-fa
cargo test -p lpvm-native-fa
cargo test -p lps-filetests -- --ignored 2>&1 | grep -E "(passed|failed)"

# ESP32 check
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

## Summary Document

Create `docs/plans/2026-04-12-fastalloc4-m3/summary.md`:

```markdown
# M3.1: Advanced Straight-Line - Summary

## Completed

### Phase 1: Pool Control
- Added `RegPool::with_capacity(n)` for testing
- Created `AllocTestBuilder` with fluent API
- Added `alloc_test().pool_size(n).vinst().expect_vinst()` pattern

### Phase 2: Spill Validation
- Enhanced render format: `; spill:`, `; reload:`, `; move:`
- `spill_simple.glsl` passes under rv32fa target
- Comprehensive spill unit tests

### Phase 3: Entry Moves
- Record entry moves when params evicted from ABI regs
- Render shows `; move: param_i0: a0 -> t1`
- `param_eviction.glsl` filetest passes

### Phase 4: Integration
- All 23 existing M2 filetests pass
- 2 new filetests added
- No regressions

## New APIs

```rust
// Test builder pattern
alloc_test()
    .pool_size(4)
    .arg_reg_limit(2)  // for M3.2
    .vinst("i0 = IConst32 10...")
    .expect_vinst("...");
```

## Validation

- 25 filetests passing (23 M2 + 2 new)
- All unit tests passing
- ESP32 firmware compiles
```

## Commit

```bash
git add -A
git commit -m "feat(fa_alloc): M3.1 advanced straight-line allocator

- Add RegPool::with_capacity(n) for spill pressure testing
- Add AllocTestBuilder with fluent API for tests
- Implement entry move recording when params evicted
- Enhance render format: ; spill:, ; reload:, ; move:
- Add spill_pressure_3regs.glsl and param_eviction.glsl tests
- All 23 M2 filetests still pass
- 2 new advanced tests passing"
```

## Move Plan to Done

```bash
mkdir -p docs/plans-done
mv docs/plans/2026-04-12-fastalloc4-m3 docs/plans-done/
```
