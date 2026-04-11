# Phase 2: Direct Calls (Clobber, Spill, Reload, Arg/Ret)

## Scope

Handle `VInst::Call` in the backward walk for direct-return calls (not sret).
Implement caller-saved register spill/reload around calls and argument/return
value placement in ABI registers.

## Code Organization Reminders

- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation Details

### 1. Pass `func_abi` into `walk_region` and `process_inst`

Currently `walk_region` doesn't receive `func_abi`. Thread it through:

```rust
pub fn walk_region(
    state: &mut WalkState,
    tree: &RegionTree,
    region_id: RegionId,
    vinsts: &[VInst],
    vreg_pool: &[VReg],
    func_abi: &crate::abi::FuncAbi,  // new
) -> Result<(), AllocError> { ... }

fn process_inst(
    state: &mut WalkState,
    idx: usize,
    vinst: &VInst,
    vreg_pool: &[VReg],
    func_abi: &crate::abi::FuncAbi,  // new
) -> Result<(), AllocError> { ... }
```

Update all callers in `walk_region` (Linear, Seq match arms) and `allocate()`.

### 2. Add `AllocError::TooManyArgs` variant

For calls with more than 8 args (overflow to stack), return an error for now:

```rust
pub enum AllocError {
    UnsupportedControlFlow,  // will be removed in phase 3/4
    UnsupportedCall,         // will be removed by this phase
    UnsupportedSelect,       // removed in phase 1
    TooManyArgs,             // new: >8 args not yet supported
    UnsupportedSret,         // new: sret calls deferred to phase 5
}
```

### 3. Implement `process_call` in `walk.rs`

Add a dedicated function for Call handling:

```rust
fn process_call(
    state: &mut WalkState,
    idx: usize,
    vinst: &VInst,
    vreg_pool: &[VReg],
    func_abi: &crate::abi::FuncAbi,
) -> Result<(), AllocError> {
    let (target, args, rets, callee_uses_sret) = match vinst {
        VInst::Call { target, args, rets, callee_uses_sret, .. } =>
            (*target, *args, *rets, *callee_uses_sret),
        _ => unreachable!(),
    };

    if callee_uses_sret {
        return Err(AllocError::UnsupportedSret); // phase 5
    }
    if args.count as usize > ARG_REGS.len() {
        return Err(AllocError::TooManyArgs);
    }

    let mut decision = String::new();

    // Step 1: Process defs (return values) — free their registers
    // In backward walk, defs are "born" here
    let ret_vregs: Vec<VReg> = rets.vregs(vreg_pool).to_vec();
    let mut ret_pregs = Vec::new();
    for &rv in &ret_vregs {
        if let Some(preg) = state.pool.home(rv) {
            state.pool.free(preg);
            ret_pregs.push((rv, preg));
        } else {
            // Dead return value — allocate to RET_REG temporarily
            let ret_idx = ret_pregs.len();
            let ret_preg = RET_REGS[ret_idx];
            ret_pregs.push((rv, ret_preg));
        }
    }

    // Step 2: Emit reloads for live vregs in caller-saved regs
    // (These execute AFTER the call — first in backward stream)
    let clobber = func_abi.call_clobbers();
    let mut clobbered_vregs: Vec<(VReg, PReg)> = Vec::new();
    for (preg, vreg) in state.pool.iter_occupied().collect::<Vec<_>>() {
        if clobber.contains_hw(preg) {
            let slot = state.spill.get_or_assign(vreg);
            let offset = -((slot as i32 + 1) * 4);
            // Emit reload (Lw) — post-call in execution
            state.pinsts.push(PInst::Lw { dst: preg, base: FP_REG, offset });
            clobbered_vregs.push((vreg, preg));
            decision.push_str(&format!(" reload v{}←[fp{}]", vreg.0, offset));
        }
    }

    // Step 3: Emit PInst::Call
    let sym_name = state.symbols_name(target);
    state.pinsts.push(PInst::Call {
        target: SymbolRef { name: sym_name },
    });

    // Step 4: Move return values to RET_REGS if needed
    for (i, &(rv, preg)) in ret_pregs.iter().enumerate() {
        let ret_reg = RET_REGS[i];
        if preg != ret_reg {
            state.pinsts.push(PInst::Mv { dst: preg, src: ret_reg });
        }
    }

    // Step 5: Resolve args and move to ARG_REGS
    let arg_vregs: Vec<VReg> = args.vregs(vreg_pool).to_vec();
    for (i, &av) in arg_vregs.iter().enumerate() {
        let arg_reg = ARG_REGS[i];
        let src = if let Some(p) = state.pool.home(av) {
            state.pool.touch(p);
            p
        } else if let Some(slot) = state.spill.has_slot(av) {
            let (p, evicted) = state.pool.alloc(av);
            if let Some(ev) = evicted {
                let ev_slot = state.spill.get_or_assign(ev);
                let offset = -((ev_slot as i32 + 1) * 4);
                state.pinsts.push(PInst::Sw { src: p, base: FP_REG, offset });
            }
            let offset = -((slot as i32 + 1) * 4);
            state.pinsts.push(PInst::Lw { dst: p, base: FP_REG, offset });
            p
        } else {
            let (p, evicted) = state.pool.alloc(av);
            if let Some(ev) = evicted {
                let ev_slot = state.spill.get_or_assign(ev);
                let offset = -((ev_slot as i32 + 1) * 4);
                state.pinsts.push(PInst::Sw { src: p, base: FP_REG, offset });
            }
            p
        };
        if src != arg_reg.hw as PReg {
            state.pinsts.push(PInst::Mv { dst: arg_reg.hw as PReg, src });
        }
    }

    // Step 6: Spill clobbered vregs (pre-call in execution — last in backward stream)
    for &(vreg, preg) in &clobbered_vregs {
        let slot = state.spill.get_or_assign(vreg);
        let offset = -((slot as i32 + 1) * 4);
        state.pinsts.push(PInst::Sw { src: preg, base: FP_REG, offset });
        // Free the register (it's clobbered by the call)
        state.pool.free(preg);
        decision.push_str(&format!(" spill v{}→[fp{}]", vreg.0, offset));
    }

    // Record trace
    let state_str = format_pool_state(&state.pool);
    state.trace.push(TraceEntry {
        vinst_idx: idx,
        vinst_mnemonic: "Call".into(),
        decision,
        register_state: state_str,
    });

    Ok(())
}
```

**Important**: The above is pseudocode showing the logical flow. The actual
implementation needs to handle `SymbolRef` construction (the walk state needs
access to `ModuleSymbols` to resolve `SymbolId` → name string). Thread
`symbols: &ModuleSymbols` through `walk_region` and `process_inst`, or store
it in `WalkState`.

### 4. Wire `process_call` into `process_inst`

Replace the `VInst::Call` rejection with a delegation:

```rust
VInst::Call { .. } => return process_call(state, idx, vinst, vreg_pool, func_abi),
```

### 5. Thread `ModuleSymbols` into WalkState

Add `symbols: &'a ModuleSymbols` to WalkState (make it generic over lifetime)
or pass it as a parameter. The call emission needs to resolve `SymbolId` to a
name string for `PInst::Call { target: SymbolRef { name } }`.

Simpler approach: pass `&ModuleSymbols` alongside `vinsts` in the walk functions.

## Tests

- Unit test: `process_call` for a 2-arg, 1-ret call produces correct PInst
  sequence (spill + arg moves + call + ret move + reload).
- Unit test: Call with no live caller-saved regs emits just arg moves + call.
- Unit test: Call with dead return value still emits call.
- Test that `callee_uses_sret = true` returns `Err(UnsupportedSret)`.

## Validate

```bash
cargo test -p lpvm-native-fa
cargo check -p lpvm-native-fa
# Test with CLI:
cargo run -p lp-cli -- shader-rv32fa lp-shader/lps-filetests/filetests/lpvm/native/native-call-control-flow.glsl 2>&1
# Should get past "calls not supported" for native_branch_helper (straight-line + call)
```
