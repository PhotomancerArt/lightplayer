# Phase 7: Update Regalloc Modules

## Scope

Mechanical updates to `greedy.rs`, `linear_scan.rs`, and `isa/rv32/alloc.rs` to use the new types:
- `VReg(u16)` instead of `lpir::VReg`
- `for_each_def()` / `for_each_use()` instead of `defs()` / `uses()`
- Access to `vreg_pool` for slice resolution

## Implementation

### 1. Update `regalloc/greedy.rs`

The changes are mechanical — replace iterator-based defs/uses with callback-based:

```rust
// OLD:
// for def in inst.defs() { ... }
// for use in inst.uses() { ... }

// NEW:
inst.for_each_def(pool, |def| {
    // ... existing logic ...
});

inst.for_each_use(pool, |use_| {
    // ... existing logic ...
});
```

Key locations to update:
- Where `defs()` is called to assign registers to destinations
- Where `uses()` is called to mark registers as live

### 2. Update `regalloc/linear_scan.rs`

Same pattern as greedy.rs:

```rust
// Replace iterator calls with callbacks
inst.for_each_def(pool, |def| {
    // allocation logic
});

inst.for_each_use(pool, |use_| {
    // liveness logic
});
```

### 3. Update `isa/rv32/alloc.rs`

The existing fast allocator in `isa/rv32/alloc.rs` needs similar updates. This is the allocator that will be replaced in M4, but it must compile with new types.

### 4. Update function signatures

Some functions may need the pool parameter added:

```rust
// OLD:
fn process_instruction(inst: &VInst, state: &mut AllocState)

// NEW:
fn process_instruction(inst: &VInst, pool: &[VReg], state: &mut AllocState)
```

## Code Organization Reminders

- These are mechanical changes — no algorithmic changes
- The goal is just to make existing allocators compile with new types
- The allocators don't need optimization, just correctness
- Add `pool` parameter where needed, pass it through call chains

## Validate

```bash
cargo check -p lpvm-native --lib
```

Check for compile errors. Fix any issues with missing pool parameters or type mismatches.

```bash
cargo test -p lpvm-native --lib -- regalloc
```

Tests should pass with the updated allocators.
