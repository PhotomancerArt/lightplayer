# Phase 3: Entry Parameter Moves

## Scope

Record entry moves when parameters get evicted from their ABI registers.

## Implementation

### 1. Track param locations in walk.rs

Modify `walk_linear` to track entry param final locations:

```rust
pub fn walk_linear(...) -> Result<AllocOutput, AllocError> {
    // ... existing setup ...
    
    // Track entry params and their ABI regs
    let entry_precolors: Vec<(VReg, PReg)> = /* existing */;
    
    // ... backward walk ...
    
    // After walk: check where each param ended up
    let mut entry_edits: Vec<(EditPoint, Edit)> = Vec::new();
    for (vreg, abi_reg) in entry_precolors {
        let final_alloc = if let Some(preg) = pool.home(vreg) {
            Alloc::Reg(preg)
        } else if let Some(slot) = spill.has_slot(vreg) {
            Alloc::Stack(slot)
        } else {
            // Param never used, no need to track
            continue;
        };
        
        let abi_alloc = Alloc::Reg(abi_reg);
        
        // If param moved from ABI reg, record entry move
        if final_alloc != abi_alloc {
            entry_edits.push((
                EditPoint::Before(0),
                Edit::Move {
                    from: abi_alloc,
                    to: final_alloc,
                },
            ));
        }
    }
    
    // Combine entry edits with other edits
    entry_edits.extend(edits);
    let final_edits = entry_edits;
    
    Ok(AllocOutput {
        // ...
        edits: final_edits,
        // ...
    })
}
```

### 2. Update render for entry moves

In `fa_alloc/render.rs`, show entry moves distinctly:

```rust
// At Before(0), show as entry move
if inst_idx == 0 && matches!(edit, Edit::Move { from: Alloc::Reg(_), to: _ }) {
    lines.push(format!("; move: param_{}: {} -> {}", 
        vreg_name, format_alloc(from), format_alloc(to)));
} else {
    lines.push(format!("; {}", format_edit(edit)));
}
```

### 3. Create param_eviction.glsl filetest

Create `lp-shader/lps-filetests/filetests/advanced/param_eviction.glsl`:

```glsl
// run: test_param_moves() == 30

int test_param_moves(int a, int b, int c, int d) {
    // With limited pool, params will need to move from ABI regs
    // Use all params to ensure they're live
    return a + b + c + d;
}

// run: test_param_moves(5, 10, 7, 8) == 30
```

### 4. Add comprehensive unit tests

```rust
#[test]
fn param_stays_in_abi_reg() {
    // Single param, stays in a0
    alloc_test()
        .pool_size(4)
        .abi_params(1)  // a in a0
        .vinst("
            i0 = Param 0
            Ret i0
        ")
        .expect_vinst("
            // No entry move - param stays in a0
            i0 = Param 0
            ; write: i0 -> a0
            Ret i0
        ");
}

#[test]
fn param_evicted_to_different_reg() {
    // Param moves from a0 to another reg
    alloc_test()
        .pool_size(2)
        .abi_params(2)  // a in a0, b in a1
        .vinst("
            i0 = Param 0  // a
            i1 = Param 1  // b
            i2 = IConst32 10
            i3 = Add32 i0, i2  // uses a, forces eviction
            i4 = Add32 i1, i3
            Ret i4
        ")
        .expect_vinst("
            ; move: param_i0: a0 -> t0
            // param i1 stays in a1
            i0 = Param 0
            ; write: i0 -> t0
            i1 = Param 1
            ; write: i1 -> a1
            i2 = IConst32 10
            ; write: i2 -> t1
            ; read: i0 <- t0
            i3 = Add32 i0, i2
            ; write: i3 -> t0
            ; read: i1 <- a1
            ; read: i3 <- t0
            i4 = Add32 i1, i3
            ; write: i4 -> t0
            Ret i4
        ");
}

#[test]
fn param_spilled_to_stack() {
    // Param goes directly to stack
    alloc_test()
        .pool_size(1)
        .abi_params(2)  // a in a0, b in a1
        .vinst("
            i0 = Param 0
            i1 = Param 1
            i2 = IConst32 10
            // Both params need to spill
            i3 = Add32 i0, i1
            Ret i3
        ")
        .expect_vinst("
            ; move: param_i0: a0 -> slot0
            ; move: param_i1: a1 -> slot1
            i0 = Param 0
            ; write: i0 -> slot0
            i1 = Param 1
            ; write: i1 -> slot1
            ; reload: slot0 -> t0
            ; reload: slot1 -> t0
            i2 = IConst32 10
            ; write: i2 -> t0
            ; read: i0 <- slot0
            ; read: i2 <- t0
            i3 = Add32 i0, i2
            ; write: i3 -> t0
            ; read: i1 <- slot1
            Ret i3
        ");
}

#[test]
fn multi_param_mixed_behavior() {
    // Some params move, some don't
    alloc_test()
        .pool_size(3)
        .abi_params(4)  // a0, a1, a2, a3
        .vinst("
            i0 = Param 0  // stays in a0
            i1 = Param 1  // stays in a1
            i2 = Param 2  // evicted to t0
            i3 = Param 3  // evicted to t1
            i4 = Add32 i0, i1  // uses first two
            i5 = Add32 i2, i3
            i6 = Add32 i4, i5
            Ret i6
        ")
        .expect_vinst("
            // a0, a1 stay
            ; move: param_i2: a2 -> t0
            ; move: param_i3: a3 -> t1
            i0 = Param 0
            ; write: i0 -> a0
            i1 = Param 1
            ; write: i1 -> a1
            i2 = Param 2
            ; write: i2 -> t0
            i3 = Param 3
            ; write: i3 -> t1
            ; read: i0 <- a0
            ; read: i1 <- a1
            i4 = Add32 i0, i1
            ; write: i4 -> a0
            ; read: i2 <- t0
            ; read: i3 <- t1
            i5 = Add32 i2, i3
            ; write: i5 -> t1
            ; read: i4 <- a0
            ; read: i5 <- t1
            i6 = Add32 i4, i5
            ; write: i6 -> t0
            Ret i6
        ");
}

#[test]
fn dead_param_no_move_needed() {
    // Param never used, no move needed
    alloc_test()
        .pool_size(2)
        .abi_params(2)
        .vinst("
            i0 = Param 0  // used
            i1 = Param 1  // dead, never used
            Ret i0
        ")
        .expect_vinst("
            ; move: param_i0: a0 -> t0
            // param i1 dead, no move
            i0 = Param 0
            ; write: i0 -> t0
            // i1 not allocated at all
            Ret i0
        ");
}

#[test]
fn param_evicted_and_reloaded_correctly() {
    // Param evicted, then reloaded when needed
    alloc_test()
        .pool_size(2)
        .abi_params(3)
        .vinst("
            i0 = Param 0  // in a0, evicted
            i1 = Param 1  // in a1
            i2 = Param 2  // in a2
            // Use i1, i2 first (evicts i0)
            i3 = Add32 i1, i2
            // Now reload i0
            i4 = Add32 i0, i3
            Ret i4
        ")
        .expect_vinst("
            ; move: param_i0: a0 -> t0
            ; spill: t0 -> slot0
            ; move: param_i1: a1 -> t0
            ; move: param_i2: a2 -> t1
            i0 = Param 0
            ; write: i0 -> slot0
            i1 = Param 1
            ; write: i1 -> t0
            i2 = Param 2
            ; write: i2 -> t1
            ; read: i1 <- t0
            ; read: i2 <- t1
            i3 = Add32 i1, i2
            ; write: i3 -> t0
            ; reload: slot0 -> t1
            ; read: i0 <- t1
            ; read: i3 <- t0
            i4 = Add32 i0, i3
            ; write: i4 -> t0
            Ret i4
        ");
}
```

## Validation

```bash
# Unit test
cargo test -p lpvm-native-fa entry_param

# Filetest
TEST_FILE=param_eviction TARGET=rv32fa.q32 cargo test -p lps-filetests -- --ignored
```

## Success Criteria

- Entry moves recorded when params evicted from ABI regs
- Render shows `; move: param_i0: a0 -> t1` format
- `param_eviction.glsl` filetest passes
