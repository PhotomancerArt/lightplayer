## Phase 1: Regalloc - Incoming Stack Param Assignment

## Scope

Modify `GreedyAlloc` to assign physical registers for incoming stack parameters instead of rejecting them.

## Code Organization Reminders

- Place helper utilities at the bottom
- Keep `Allocation` struct definition at the top of `regalloc/mod.rs`
- Add tests at the bottom of `regalloc/greedy.rs` in `mod tests`

## Implementation Details

### 1. Update `Allocation` struct

**File**: `lp-shader/lpvm-native/src/regalloc/mod.rs`

Add field to track incoming stack parameters:

```rust
pub struct Allocation {
    pub vreg_to_phys: Vec<Option<PhysReg>>,
    pub clobbered: BTreeSet<PhysReg>,
    pub spill_slots: Vec<VReg>,
    /// Incoming parameters passed on stack: (vreg, offset_from_s0)
    pub incoming_stack_params: Vec<(VReg, i32)>,
}
```

### 2. Modify `allocate_with_func_abi`

**File**: `lp-shader/lpvm-native/src/regalloc/greedy.rs`

Around line 78-93, replace the `ArgLoc::Stack` error case:

```rust
// OLD (rejects stack params):
ArgLoc::Stack { .. } => {
    return Err(NativeError::TooManyArgs(slots));
}

// NEW (assigns register, tracks for prologue load):
ArgLoc::Stack { offset, .. } => {
    // Find first allocatable register not already used
    let used: BTreeSet<PhysReg> = vreg_to_phys[..i]
        .iter()
        .filter_map(|x| *x)
        .collect();
    
    let pick = alloca_list
        .iter()
        .find(|r| !used.contains(r))
        .copied()
        .ok_or(NativeError::TooManyArgs(slots))?;
    
    vreg_to_phys[i] = Some(pick);
    incoming_stack_params.push((VReg(i as u32), offset));
}
```

Initialize `incoming_stack_params` at the top of the function:

```rust
let mut incoming_stack_params: Vec<(VReg, i32)> = Vec::new();
```

Include it in the returned `Allocation`:

```rust
Ok(Allocation {
    vreg_to_phys,
    clobbered,
    spill_slots,
    incoming_stack_params,
})
```

### 3. Advance next_alloca past param registers

After processing all params, advance `next_alloca` to skip the registers we just used:

```rust
// After param assignment loop, update next_alloca
for p_opt in vreg_to_phys.iter().take(slots) {
    if let Some(p) = p_opt {
        if let Some(pos) = alloca_list.iter().position(|r| r == p) {
            next_alloca = next_alloca.max(pos + 1);
        }
    }
}
```

## Tests

Add test in `regalloc/greedy.rs` `mod tests`:

```rust
#[test]
fn stack_params_get_registers() {
    // Create function with 10 param slots (vmctx + 9 user params)
    let f = IrFunction {
        name: String::from("many_params"),
        is_entry: true,
        vmctx_vreg: VReg(0),
        param_count: 9, // 9 user params = 10 total slots
        return_types: vec![lpir::IrType::I32],
        vreg_types: vec![lpir::IrType::I32; 11], // 0..10
        slots: vec![],
        body: vec![
            Op::Return {
                values: lpir::types::VRegRange { start: 0, count: 1 },
            },
        ],
        vreg_pool: vec![VReg(10)],
    };
    
    let ir = ir_single(f.clone());
    let mabi = ModuleAbi::from_ir_and_sig(&ir, &test_sig());
    let vinsts = lower_ops(&f, &ir, &mabi, lpir::FloatMode::Q32).unwrap();
    
    // Build ABI with 10 param slots
    let sig = test_sig_many_params(9); // 9 user params
    let func_abi = func_abi_rv32(&sig, 10);
    
    let alloc = GreedyAlloc::new()
        .allocate_with_func_abi(&f, &vinsts, &func_abi)
        .expect("should handle 10 params");
    
    // Verify all 10 params have registers
    for i in 0..10 {
        assert!(alloc.vreg_to_phys[i].is_some(), "param {} should have reg", i);
    }
    
    // Verify stack params tracked (params 8 and 9 are on stack in ILP32 ABI)
    assert!(!alloc.incoming_stack_params.is_empty(), "should have stack params");
}
```

## Validate

```bash
cd /Users/yona/dev/photomancer/feature/lightplayer-native/lp-shader
cargo test -p lpvm-native --lib -- --test-threads=1
```

Expected: All 88 existing tests pass, new test passes.
