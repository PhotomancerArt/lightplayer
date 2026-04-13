# Phase 2: WalkState + process_inst

## Scope

Implement the backward walk with real allocation decisions. `WalkState` threads
mutable state through the region tree walk. `process_inst` handles the
per-instruction allocation logic (defs free regs, uses allocate regs).

## Code Organization Reminders

- `WalkState` and `process_inst` live in `walk.rs`
- The VInst→PInst translation match is in phase 3 (this phase uses a helper
  that takes resolved PRegs and returns `Vec<PInst>`)
- Tests first, helpers at bottom

## Implementation Details

### `WalkState` in `fa_alloc/walk.rs`

Replace the existing `walk_region_stub` with real allocation logic.

```rust
use alloc::vec::Vec;
use crate::abi::FuncAbi;
use crate::fa_alloc::spill::SpillAlloc;
use crate::fa_alloc::trace::{AllocTrace, TraceEntry};
use crate::region::{Region, RegionId, RegionTree, REGION_ID_NONE};
use crate::rv32::gpr::{self, PReg, FP_REG};
use crate::rv32::inst::PInst;
use crate::vinst::{VInst, VReg};

pub struct WalkState {
    pub pool: RegPool,
    pub spill: SpillAlloc,
    pub trace: AllocTrace,
    pub pinsts: Vec<PInst>,
}

impl WalkState {
    pub fn new(num_vregs: usize) -> Self {
        Self {
            pool: RegPool::new(),
            spill: SpillAlloc::new(num_vregs),
            trace: AllocTrace::new(),
            pinsts: Vec::new(),
        }
    }
}
```

### `walk_region` — replaces `walk_region_stub`

```rust
pub fn walk_region(
    state: &mut WalkState,
    tree: &RegionTree,
    region_id: RegionId,
    vinsts: &[VInst],
    vreg_pool: &[VReg],
) -> Result<(), AllocError> {
    if region_id == REGION_ID_NONE {
        return Ok(());
    }
    let region = &tree.nodes[region_id as usize];
    match region {
        Region::Linear { start, end } => {
            for i in (*start..*end).rev() {
                process_inst(state, i as usize, &vinsts[i as usize], vreg_pool)?;
            }
            Ok(())
        }
        Region::Seq { children_start, child_count } => {
            let start = *children_start as usize;
            let end = start + *child_count as usize;
            // Walk children in reverse (backward)
            for &child_id in tree.seq_children[start..end].iter().rev() {
                walk_region(state, tree, child_id, vinsts, vreg_pool)?;
            }
            Ok(())
        }
        Region::IfThenElse { .. } | Region::Loop { .. } => {
            Err(AllocError::UnsupportedControlFlow)
        }
    }
}
```

### `process_inst` — per-instruction allocation

```rust
fn process_inst(
    state: &mut WalkState,
    idx: usize,
    vinst: &VInst,
    vreg_pool: &[VReg],
) -> Result<(), AllocError> {
    // Skip labels
    if matches!(vinst, VInst::Label(..)) {
        return Ok(());
    }

    // Reject unsupported instructions
    match vinst {
        VInst::Call { .. } => return Err(AllocError::UnsupportedCall),
        VInst::Select32 { .. } => return Err(AllocError::UnsupportedSelect),
        VInst::Br { .. } | VInst::BrIf { .. } => {
            return Err(AllocError::UnsupportedControlFlow)
        }
        _ => {}
    }

    let mut decision = String::new();

    // 1. Defs: in backward walk, this is where the value dies — free its reg
    let mut def_pregs = Vec::new();
    vinst.for_each_def(vreg_pool, |d| {
        if let Some(preg) = state.pool.home(d) {
            state.pool.free(preg);
            def_pregs.push((d, preg));
        }
    });

    // 2. Uses: in backward walk, this is where the value is born — ensure in reg
    let mut use_pregs = Vec::new();
    vinst.for_each_use(vreg_pool, |u| {
        use_pregs.push(u);
    });
    let resolved_uses = resolve_uses(state, &use_pregs, &mut decision)?;

    // 3. Emit PInst (phase 3 fills this in fully)
    let emitted = emit_vinst(state, idx, vinst, &def_pregs, &resolved_uses, vreg_pool)?;
    state.pinsts.extend(emitted);

    // 4. Record trace
    state.trace.push(TraceEntry {
        vinst_idx: idx,
        vinst_mnemonic: vinst.mnemonic().into(),
        decision,
        register_state: format_pool_state(&state.pool),
    });

    Ok(())
}
```

### `resolve_uses` — allocate PRegs for use-vregs

```rust
fn resolve_uses(
    state: &mut WalkState,
    use_vregs: &[VReg],
    decision: &mut String,
) -> Result<Vec<PReg>, AllocError> {
    let mut resolved = Vec::with_capacity(use_vregs.len());
    for &vreg in use_vregs {
        let preg = if let Some(p) = state.pool.home(vreg) {
            // Already in a register
            state.pool.touch(p);
            p
        } else if let Some(slot) = state.spill.has_slot(vreg) {
            // Spilled — reload
            let (p, evicted) = state.pool.alloc(vreg);
            if let Some(ev) = evicted {
                handle_eviction(state, ev, decision);
            }
            let offset = -((slot as i32 + 1) * 4);
            state.pinsts.push(PInst::Lw {
                dst: p,
                base: FP_REG,
                offset,
            });
            decision.push_str(&format!(" reload v{}→{}", vreg.0, gpr::reg_name(p)));
            p
        } else {
            // First time seeing this vreg — allocate
            let (p, evicted) = state.pool.alloc(vreg);
            if let Some(ev) = evicted {
                handle_eviction(state, ev, decision);
            }
            decision.push_str(&format!(" v{}→{}", vreg.0, gpr::reg_name(p)));
            p
        };
        resolved.push(preg);
    }
    Ok(resolved)
}
```

### `handle_eviction` — spill an evicted vreg

```rust
fn handle_eviction(state: &mut WalkState, evicted: VReg, decision: &mut String) {
    let slot = state.spill.get_or_assign(evicted);
    let offset = -((slot as i32 + 1) * 4);
    // In backward walk, a spill store appears AFTER the current instruction
    // (earlier in execution). We're building backward, so push it now.
    // TODO: for correctness, the spill store needs the PReg the evicted vreg
    // WAS in. We need to capture that before the eviction happens in RegPool.
    // For now, track evicted_preg in the alloc() return.
    decision.push_str(&format!(" evict v{} to [fp{}]", evicted.0, offset));
}
```

### `AllocError`

Add to or reuse from `rv32::alloc`:

```rust
pub enum AllocError {
    UnsupportedControlFlow,
    UnsupportedCall,
    UnsupportedSelect,
}
```

This can go in `fa_alloc/mod.rs` or a shared error location.

### Tests

```rust
#[test]
fn walk_linear_tracks_regs() {
    // Build a Linear region with: IConst32 v0=42, Ret v0
    // Walk backward: Ret first (allocate v0), then IConst32 (free v0)
    // Verify: pinsts contain Li + Mv-to-ret-reg + Ret
}

#[test]
fn walk_seq_threads_state() {
    // Build a Seq of two Linear regions
    // Verify: register state is continuous across the boundary
}

#[test]
fn walk_rejects_control_flow() {
    // IfThenElse region → AllocError::UnsupportedControlFlow
}
```

## Validate

```bash
cargo test -p lpvm-native-fa --lib -- fa_alloc
```
