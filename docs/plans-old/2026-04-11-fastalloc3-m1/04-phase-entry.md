# Phase 4: Entry Point + Frame Wrapping

## Scope

Implement `fa_alloc::allocate` — the public entry point that builds WalkState,
runs the backward walk, applies param fixups, wraps with FrameSetup/FrameTeardown,
and returns `AllocResult`.

## Code Organization Reminders

- `allocate` is the public API in `fa_alloc/mod.rs`
- `AllocResult` defined in `fa_alloc/mod.rs`
- Keep `run_shell` temporarily (mark deprecated) until M2 switches over
- Tests first, helpers at bottom

## Implementation Details

### `AllocResult` and `allocate` in `fa_alloc/mod.rs`

```rust
use alloc::vec::Vec;
use crate::abi::FuncAbi;
use crate::lower::LoweredFunction;
use crate::rv32::gpr;
use crate::rv32::inst::PInst;

pub mod liveness;
pub mod spill;
pub mod trace;
pub mod walk;

pub use walk::AllocError;

pub struct AllocResult {
    pub pinsts: Vec<PInst>,
    pub trace: trace::AllocTrace,
    pub spill_slots: u32,
}

pub fn allocate(
    lowered: &LoweredFunction,
    func_abi: &FuncAbi,
) -> Result<AllocResult, AllocError> {
    let num_vregs = lowered.vreg_pool.len().max(
        // Scan for max vreg index to size the spill array
        max_vreg_index(&lowered.vinsts, &lowered.vreg_pool)
    );

    let mut state = walk::WalkState::new(num_vregs);

    // Run backward walk over region tree
    walk::walk_region(
        &mut state,
        &lowered.region_tree,
        lowered.region_tree.root,
        &lowered.vinsts,
        &lowered.vreg_pool,
    )?;

    // Param fixups: if any precolored vreg isn't in its ARG_REG, emit Mv
    apply_param_fixups(&mut state, func_abi, &lowered.vreg_pool);

    // Reverse: we built backward, execution order is forward
    state.pinsts.reverse();
    state.trace.reverse();

    // Wrap with frame setup/teardown
    let spill_slots = state.spill.total_slots();
    let mut pinsts = Vec::with_capacity(state.pinsts.len() + 2);
    pinsts.push(PInst::FrameSetup { spill_slots });
    pinsts.extend(state.pinsts);
    pinsts.push(PInst::FrameTeardown { spill_slots });

    Ok(AllocResult {
        pinsts,
        trace: state.trace,
        spill_slots,
    })
}
```

### `apply_param_fixups`

After the backward walk, check each precolored param vreg. If it's currently
in a register that isn't its designated ARG_REG, emit a Mv.

```rust
fn apply_param_fixups(
    state: &mut walk::WalkState,
    func_abi: &FuncAbi,
    vreg_pool: &[VReg],
) {
    for (vreg_idx, abi_preg) in func_abi.precolors() {
        let vreg = VReg(*vreg_idx as u16);
        let want = abi_preg.hw;
        if let Some(have) = state.pool.home(vreg) {
            if have != want {
                // Emit Mv from ARG_REG to wherever the walk put it
                // (This goes at the start of the function, before any use)
                state.pinsts.push(PInst::Mv { dst: have, src: want });
            }
        }
    }
}
```

### `max_vreg_index` helper

```rust
fn max_vreg_index(vinsts: &[VInst], pool: &[VReg]) -> usize {
    let mut m = 0usize;
    for inst in vinsts {
        inst.for_each_use(pool, |u| m = m.max(u.0 as usize + 1));
        inst.for_each_def(pool, |d| m = m.max(d.0 as usize + 1));
    }
    m
}
```

### Tests

```rust
#[test]
fn allocate_simple_iconst_ret() {
    // IConst32 v0=42, Ret v0
    // Expected: FrameSetup, Li, (maybe Mv to a0), Ret, FrameTeardown
    let lowered = build_lowered_iconst_ret(42);
    let func_abi = build_void_abi();
    let result = allocate(&lowered, &func_abi).unwrap();

    assert!(matches!(result.pinsts[0], PInst::FrameSetup { .. }));
    assert!(matches!(result.pinsts.last(), Some(PInst::FrameTeardown { .. })));
    assert!(result.pinsts.iter().any(|p| matches!(p, PInst::Li { imm: 42, .. })));
    assert!(result.pinsts.iter().any(|p| matches!(p, PInst::Ret)));
}

#[test]
fn allocate_add_two_params() {
    // Params v1, v2 in a0, a1. Add32 v3=v1+v2, Ret v3.
    // Expected: FrameSetup, Add, (Mv to a0 if needed), Ret, FrameTeardown
    let lowered = build_lowered_add_params();
    let func_abi = build_two_int_params_abi();
    let result = allocate(&lowered, &func_abi).unwrap();

    assert!(matches!(result.pinsts[0], PInst::FrameSetup { .. }));
    assert!(result.pinsts.iter().any(|p| matches!(p, PInst::Add { .. })));
}

#[test]
fn allocate_returns_spill_count() {
    // Create enough vregs to force spills
    // Verify spill_slots > 0 in AllocResult
}

#[test]
fn allocate_rejects_control_flow() {
    // LoweredFunction with IfThenElse region
    // Expected: AllocError::UnsupportedControlFlow
}
```

## Validate

```bash
cargo test -p lpvm-native --lib -- fa_alloc
```
