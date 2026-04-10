# M3: Allocator Core with Trace

## Scope of Work

Implement the backward-walk allocator that converts VInst to PhysInst, with the trace system for debugging.

## Files

```
lp-shader/lpvm-native/src/isa/rv32fa/
├── alloc.rs                 # NEW: backward-walk allocator
├── trace.rs                 # NEW: AllocTrace system
```

## Implementation Details

### 1. Trace System in `trace.rs`

```rust
//! Structured trace for debugging allocator decisions.

use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;
use crate::vinst::VInst;
use crate::isa::rv32fa::inst::PhysInst;

pub struct AllocTrace {
    entries: Vec<TraceEntry>,
}

pub struct TraceEntry {
    pub vinst_idx: usize,
    pub lpir_idx: Option<u32>,
    pub vinst: VInst,
    pub decision: String,
    pub physinsts: Vec<PhysInst>,
}

impl AllocTrace {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn push(&mut self, entry: TraceEntry) {
        self.entries.push(entry);
    }

    /// Reverse entries to forward order (allocator walks backward).
    pub fn reverse(&mut self) {
        self.entries.reverse();
    }

    /// Format as human-readable table.
    pub fn format_table(&self, func_name: &str) -> String {
        // Produce:
        // === fastalloc: test() ===
        // VInst | LPIR  | Instruction | Decision | PhysInst(s)
        // ...
    }
}

impl TraceEntry {
    pub fn new(
        vinst_idx: usize,
        lpir_idx: Option<u32>,
        vinst: VInst,
        decision: String,
        physinsts: Vec<PhysInst>,
    ) -> Self {
        Self { vinst_idx, lpir_idx, vinst, decision, physinsts }
    }
}
```

### 2. Allocator in `alloc.rs`

```rust
//! Backward-walk register allocator for straight-line code.

use alloc::vec::Vec;
use alloc::string::String;
use crate::error::NativeError;
use crate::vinst::{VInst, VReg};
use crate::abi::{FuncAbi, PReg};
use crate::isa::rv32fa::inst::{PhysInst, PhysReg};
use crate::isa::rv32fa::trace::{AllocTrace, TraceEntry};

pub struct AllocResult {
    pub physinsts: Vec<PhysInst>,
    pub trace: AllocTrace,
}

pub struct FastAlloc;

impl FastAlloc {
    pub fn allocate(
        vinsts: &[VInst],
        num_vregs: usize,
        abi: &FuncAbi,
    ) -> Result<AllocResult, NativeError> {
        // Check for control flow
        if has_control_flow(vinsts) {
            return Err(NativeError::FastallocUnsupportedControlFlow {
                ir_function_name: abi.name.clone(),
                message: "Fast allocator only supports straight-line code".into(),
                trace: None,
            });
        }

        let mut state = WalkState::new(num_vregs, abi);
        let mut trace = AllocTrace::new();

        // Backward walk
        for (pos, vinst) in vinsts.iter().enumerate().rev() {
            let entry = state.process_instruction(pos, vinst)?;
            trace.push(entry);
        }

        // Reverse to forward order
        trace.reverse();

        // Build function: prologue + body + epilogue
        let physinsts = build_function(&trace, abi);

        Ok(AllocResult { physinsts, trace })
    }
}

/// Walk state during backward allocation.
struct WalkState {
    /// Current home of each vreg: Some(preg) or None (on stack/spilled).
    vreg_home: Vec<Option<PhysReg>>,
    /// Which vreg is in each preg (for conflict detection).
    preg_occupant: [Option<VReg>; 32],
    /// Spill slot for each vreg (assigned on first eviction).
    vreg_spill: Vec<Option<u32>>,
    next_spill_slot: u32,
    /// ABI for arg/ret register mapping.
    abi: FuncAbi,
    /// Is this an SRET function (returns via pointer).
    is_sret: bool,
}

impl WalkState {
    fn new(num_vregs: usize, abi: &FuncAbi) -> Self {
        // Initialize vreg_home for parameters based on ABI
        // Register params -> assigned preg
        // Stack params -> None (will be loaded in prologue)
    }

    fn process_instruction(
        &mut self,
        pos: usize,
        vinst: &VInst,
    ) -> Result<TraceEntry, NativeError> {
        let mut physinsts = Vec::new();
        let mut decision = String::new();

        match vinst {
            VInst::IConst32 { dst, val, .. } => {
                // No register assigned; LoadImm generated at use sites
                decision.push_str(&format!("v{}: remat({})", dst.0, val));
            }

            VInst::Add32 { dst, src1, src2, .. } => {
                // Process uses (early): ensure src1 and src2 are in registers
                let p1 = self.ensure_reg_for_use(*src1, &mut physinsts)?;
                let p2 = self.ensure_reg_for_use(*src2, &mut physinsts)?;

                // Allocate destination
                let pd = self.alloc_reg_for_def(*dst)?;

                decision.push_str(&format!(
                    "v{}->{}, v{}->{}, v{}->{}",
                    src1.0, reg_name(p1),
                    src2.0, reg_name(p2),
                    dst.0, reg_name(pd)
                ));

                physinsts.push(PhysInst::Add32 {
                    dst: pd,
                    src1: p1,
                    src2: p2,
                });

                // Free destination register (will be re-allocated if used again)
                self.free_reg(pd);
            }

            VInst::Call { args, rets, callee_uses_sret, .. } => {
                // Handle call clobbers: spill live caller-saved regs
                for preg in caller_saved_int() {
                    if let Some(vreg) = self.preg_occupant[preg as usize] {
                        if self.is_live(vreg) {
                            let slot = self.spill_vreg(vreg)?;
                            physinsts.push(PhysInst::Store32 {
                                src: preg,
                                base: FP_REG,
                                offset: -(slot as i32 + 1) * 4,
                            });
                            decision.push_str(&format!(" spill v{} to [fp-{}]", vreg.0, (slot+1)*4));
                        }
                    }
                }

                // Move args to ABI registers
                for (i, arg) in args.iter().enumerate() {
                    let want = arg_reg(i, *callee_uses_sret);
                    let have = self.ensure_reg_for_use(*arg, &mut physinsts)?;
                    if have != want {
                        physinsts.push(PhysInst::Mov32 { dst: want, src: have });
                    }
                }

                physinsts.push(PhysInst::Call { target: ... });

                // Reload spilled regs
                for preg in caller_saved_int().rev() {
                    if let Some(vreg) = self.preg_occupant[preg as usize] {
                        if self.is_live(vreg) {
                            let slot = self.vreg_spill[vreg.0 as usize].unwrap();
                            physinsts.push(PhysInst::Load32 {
                                dst: preg,
                                base: FP_REG,
                                offset: -(slot as i32 + 1) * 4,
                            });
                        }
                    }
                }

                // Assign return values
                for (i, ret) in rets.iter().enumerate() {
                    let preg = ret_reg(i);
                    self.assign_vreg_to_preg(*ret, preg);
                }
            }

            VInst::Ret { vals, .. } => {
                // Move return values to ABI registers
                for (i, val) in vals.iter().enumerate() {
                    let want = ret_reg(i);
                    let have = self.ensure_reg_for_use(*val, &mut physinsts)?;
                    if have != want {
                        physinsts.push(PhysInst::Mov32 { dst: want, src: have });
                    }
                }
                physinsts.push(PhysInst::Ret);
            }

            // ... all other VInst variants
        }

        Ok(TraceEntry::new(
            pos,
            vinst.src_op(),
            vinst.clone(),
            decision,
            physinsts,
        ))
    }

    fn ensure_reg_for_use(
        &mut self,
        vreg: VReg,
        reloads: &mut Vec<PhysInst>,
    ) -> Result<PhysReg, NativeError> {
        // If vreg is in a register, return it.
        // If vreg is spilled, reload into a free register (or evict LRU).
        // If vreg has no home (IConst32), generate LoadImm into a free register.
    }

    fn alloc_reg_for_def(&mut self, vreg: VReg) -> Result<PhysReg, NativeError> {
        // Allocate a free register for the def (or evict LRU).
        // Record assignment.
    }

    fn spill_vreg(&mut self, vreg: VReg) -> Result<u32, NativeError> {
        // Assign next spill slot if first time, record.
    }

    fn free_reg(&mut self, preg: PhysReg) {
        // Mark register as free, remove from preg_occupant.
    }
}

fn build_function(trace: &AllocTrace, abi: &FuncAbi) -> Vec<PhysInst> {
    // Flatten all physinsts from trace entries
    // Prepend FrameSetup
    // Append FrameTeardown
}
```

### 3. Helper Functions

```rust
fn has_control_flow(vinsts: &[VInst]) -> bool {
    vinsts.iter().any(|v| matches!(v, VInst::Br { .. } | VInst::BrIf { .. }))
}

fn arg_reg(i: usize, callee_uses_sret: bool) -> PhysReg {
    // Return a0-a7 (skipping a0 if callee_uses_sret)
}

fn ret_reg(i: usize) -> PhysReg {
    // Return a0-a1
}

fn caller_saved_int() -> impl Iterator<Item = PhysReg> {
    // Return caller-saved registers in some order
}

fn reg_name(preg: PhysReg) -> &'static str {
    // Map 10 -> "a0", etc.
}
```

### 4. Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::rv32fa::vinst_parser::parse_vinsts;
    use crate::isa::rv32fa::physinst_fmt::fmt_physinsts;

    #[test]
    fn test_alloc_simple_add() {
        let vinsts = parse_vinsts("
            v0 = IConst32 1
            v1 = IConst32 2
            v2 = Add32 v0, v1
            Ret v2
        ").unwrap();

        let abi = test_abi_noargs();
        let result = FastAlloc::allocate(&vinsts, 3, &abi).unwrap();

        let output = fmt_physinsts(&result.physinsts);
        // Should look like:
        // FrameSetup { spill_slots: 0 }
        // a0 = LoadImm 1
        // a1 = LoadImm 2
        // a0 = Add32 a0, a1
        // Ret
        // FrameTeardown { spill_slots: 0 }
    }

    #[test]
    fn test_alloc_rejects_control_flow() {
        let vinsts = vec![
            VInst::Br { target: 0, src_op: None },
        ];
        let abi = test_abi_noargs();
        let err = FastAlloc::allocate(&vinsts, 0, &abi).unwrap_err();
        assert!(matches!(err, NativeError::FastallocUnsupportedControlFlow { .. }));
    }

    #[test]
    fn test_trace_is_populated() {
        // After allocation, trace.entries should have one entry per VInst
    }
}
```

## Validate

```bash
cd lp-shader/lpvm-native
cargo test -p lpvm-native --lib -- rv32fa::alloc
```

All allocator tests should pass. Simple cases (add, iconst, call, ret) should work correctly.
