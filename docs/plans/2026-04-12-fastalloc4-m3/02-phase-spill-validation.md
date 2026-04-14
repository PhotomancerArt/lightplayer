# Phase 2: Spill Logic Validation

## Scope

Verify spill/reload logic works correctly with the existing `spill_simple.glsl` filetest.

## Implementation

### 1. Update render format for spills

In `fa_alloc/render.rs`, enhance edit rendering:

```rust
fn format_edit(edit: &Edit) -> String {
    match edit {
        Edit::Move { from, to } => {
            match (from, to) {
                (Alloc::Stack(slot), Alloc::Reg(preg)) => {
                    format!("; reload: slot{} -> {}", slot, format_preg(*preg))
                }
                (Alloc::Reg(preg), Alloc::Stack(slot)) => {
                    format!("; spill: {} -> slot{}", format_preg(*preg), slot)
                }
                (Alloc::Reg(from_preg), Alloc::Reg(to_preg)) => {
                    format!("; move: {} -> {}", format_preg(*from_preg), format_preg(*to_preg))
                }
                _ => format!("; move: {:?} -> {:?}", from, to),
            }
        }
    }
}
```

### 2. Run spill_simple.glsl filetest

```bash
TEST_FILE=spill_simple TARGET=rv32fa.q32 cargo test -p lps-filetests -- --ignored
```

### 3. Debug any failures

If test fails:
1. Check if `UnsupportedControlFlow` is returned (shouldn't be, spill_simple is linear)
2. Check output with `--show-alloc` or env var
3. Compare actual vs expected
4. Fix any allocator bugs

### 4. Add comprehensive spill unit tests

```rust
#[test]
fn spill_single_value() {
    // One value spilled and reloaded
    alloc_test()
        .pool_size(2)
        .vinst("
            i0 = IConst32 10
            i1 = IConst32 20
            i2 = IConst32 30
            Ret i2
        ")
        .expect_vinst("...");
}

#[test]
fn spill_chain_multiple() {
    // Multiple spills in chain
    alloc_test()
        .pool_size(2)
        .vinst("
            i0 = IConst32 1
            i1 = IConst32 2
            i2 = IConst32 3
            i3 = IConst32 4
            i4 = Add32 i0, i1
            Ret i4
        ")
        .expect_vinst("...");
}

#[test]
fn spill_diamond_pattern() {
    // Value used in two places (diamond)
    alloc_test()
        .pool_size(2)
        .vinst("
            i0 = IConst32 10
            i1 = IConst32 20
            i2 = Add32 i0, i1
            i3 = Sub32 i0, i1
            i4 = Add32 i2, i3
            Ret i4
        ")
        .expect_vinst("...");
}

#[test]
fn spill_eviction_lru_policy() {
    // Test LRU: most recently used stays, oldest evicted
    alloc_test()
        .pool_size(3)
        .vinst("
            i0 = IConst32 1
            i1 = IConst32 2
            i2 = IConst32 3
            // Touch i1 (making i0 oldest)
            i3 = Add32 i0, i1
            // i0 should be evicted, not i1 or i2
            i4 = IConst32 4
            i5 = Add32 i1, i2
            Ret i5
        ")
        .expect_vinst("
            // Verify i0 was spilled (evicted), i1 stayed
        ");
}

#[test]
fn spill_reload_correct_slot() {
    // Ensure we reload from correct slot
    alloc_test()
        .pool_size(2)
        .vinst("
            i0 = IConst32 10  // spilled to slot0
            i1 = IConst32 20  // stays in reg
            i2 = IConst32 30  // spills i0
            // reload i0 from slot0
            i3 = Add32 i0, i2
            Ret i3
        ")
        .expect_vinst("
            i0 = IConst32 10
            ; write: i0 -> t0
            ; spill: t0 -> slot0
            i1 = IConst32 20
            ; write: i1 -> t1
            ; reload: slot0 -> t0
            i2 = IConst32 30
            ; write: i2 -> t0
            ; read: i0 <- slot0
            ; read: i2 <- t0
            i3 = Add32 i0, i2
            ; write: i3 -> t0
            Ret i3
        ");
}

#[test]
fn spill_no_reload_for_dead_value() {
    // Dead value shouldn't be reloaded
    alloc_test()
        .pool_size(1)
        .vinst("
            i0 = IConst32 10  // spilled
            i1 = IConst32 20  // kills i0
            // i0 never used again
            Ret i1
        ")
        .expect_vinst("
            // No reload of i0 since it's dead
        ");
}
```

## Validation

```bash
# Unit tests
cargo test -p lpvm-native-fa spill

# Filetest
TEST_FILE=spill_simple TARGET=rv32fa.q32 cargo test -p lps-filetests -- --ignored

# All existing tests must still pass
cargo test -p lpvm-native-fa fa_alloc::
```

## Success Criteria

- `spill_simple.glsl` passes under rv32fa target
- Unit tests show correct spill/reload patterns
- No regressions in existing tests
