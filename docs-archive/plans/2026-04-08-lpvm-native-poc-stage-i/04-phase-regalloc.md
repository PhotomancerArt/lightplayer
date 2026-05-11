## Phase 4: RegAlloc Interface + Greedy Allocator

### Scope

Define `RegAlloc` trait, `Allocation` struct, and `GreedyAlloc` implementation. Integrate with ABI: use `CALLER_SAVED` list for clobber tracking.

### Implementation details

**`regalloc/mod.rs`:**

```rust
use crate::types::{NativeType, PhysReg};
use crate::vinst::{VInst, VReg};
use crate::isa::rv32::abi::{CALLER_SAVED, ALLOCA_REGS};

pub trait RegAlloc {
    fn allocate(&self, vinsts: &[VInst], vreg_info: &VRegInfo) -> Allocation;
}

pub struct VRegInfo {
    pub count: usize,
    pub types: alloc::vec::Vec<NativeType>,
}

/// Register allocation result
#[derive(Debug, Clone)]
pub struct Allocation {
    /// vreg index -> phys reg or None (spill)
    pub vreg_to_phys: alloc::vec::Vec<Option<PhysReg>>,
    /// Which registers are clobbered by this function's calls
    pub clobbered: alloc::collections::BTreeSet<PhysReg>,
}

/// Set of clobbered registers from Call VInsts
#[derive(Default)]
pub struct ClobberSet {
    regs: alloc::collections::BTreeSet<PhysReg>,
}

impl ClobberSet {
    pub fn add_call(&mut self) {
        // All caller-saved registers are clobbered by a call
        for &reg in CALLER_SAVED {
            self.regs.insert(reg);
        }
    }
    
    pub fn into_set(self) -> alloc::collections::BTreeSet<PhysReg> {
        self.regs
    }
}
```

**`regalloc/greedy.rs`:**

```rust
use super::*;

pub struct GreedyAlloc;

impl GreedyAlloc {
    pub fn new() -> Self { Self }
}

impl RegAlloc for GreedyAlloc {
    fn allocate(&self, vinsts: &[VInst], vreg_info: &VRegInfo) -> Allocation {
        let mut vreg_to_phys: alloc::vec::Vec<Option<PhysReg>> = 
            alloc::vec::Vec::with_capacity(vreg_info.count);
        let mut clobber = ClobberSet::default();
        let mut next_reg_idx = 0usize;
        
        for inst in vinsts {
            // Track clobbers from calls
            if matches!(inst, VInst::Call { .. }) {
                clobber.add_call();
            }
            
            // Simple round-robin assignment (no liveness analysis in greedy)
            // TODO(phase-4): proper liveness for spilling decisions
            for vreg in inst.defs() {
                if vreg_to_phys.get(vreg.0 as usize).copied().flatten().is_none() {
                    // Assign new register
                    let phys = ALLOCA_REGS[next_reg_idx % ALLOCA_REGS.len()];
                    next_reg_idx += 1;
                    
                    // Ensure vec is large enough
                    while vreg_to_phys.len() <= vreg.0 as usize {
                        vreg_to_phys.push(None);
                    }
                    vreg_to_phys[vreg.0 as usize] = Some(phys);
                }
            }
        }
        
        Allocation {
            vreg_to_phys,
            clobbered: clobber.into_set(),
        }
    }
}
```

**VInst helpers** (in `vinst.rs`, bottom):

```rust
impl VInst {
    /// Defined vregs (written by this instruction)
    pub fn defs(&self) -> impl Iterator<Item = VReg> {
        let mut result = alloc::vec::Vec::new();
        match self {
            VInst::Add32 { dst, .. } | VInst::Sub32 { dst, .. } | 
            VInst::Mul32 { dst, .. } | VInst::Load32 { dst, .. } |
            VInst::IConst32 { dst, .. } => result.push(*dst),
            VInst::Call { rets, .. } => result.extend(rets.iter().cloned()),
            _ => {}
        }
        result.into_iter()
    }
    
    /// Used vregs (read by this instruction)
    pub fn uses(&self) -> impl Iterator<Item = VReg> {
        // ... similar pattern
        alloc::vec::Vec::new().into_iter()
    }
}
```

### Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::vinst::*;
    
    #[test]
    fn test_clobber_tracking() {
        let vinsts = vec![
            VInst::Call { target: SymbolRef { name: "f".into() }, args: vec![], rets: vec![] },
        ];
        let info = VRegInfo { count: 0, types: vec![] };
        let alloc = GreedyAlloc::new().allocate(&vinsts, &info);
        
        // Call clobbers all caller-saved registers
        for reg in CALLER_SAVED {
            assert!(alloc.clobbered.contains(reg), "reg {} not clobbered", reg);
        }
        
        // Callee-saved not clobbered
        for reg in CALLEE_SAVED {
            assert!(!alloc.clobbered.contains(reg), "reg {} incorrectly clobbered", reg);
        }
    }
    
    #[test]
    fn test_simple_allocation() {
        let v0 = VReg(0);
        let v1 = VReg(1);
        let vinsts = vec![
            VInst::Add32 { dst: v0, src1: v0, src2: v1 },
        ];
        let info = VRegInfo {
            count: 2,
            types: vec![NativeType::I32, NativeType::I32],
        };
        let alloc = GreedyAlloc::new().allocate(&vinsts, &info);
        
        assert!(alloc.vreg_to_phys[0].is_some(), "v0 not assigned");
        assert!(alloc.vreg_to_phys[1].is_none(), "v1 not used (only defined), not assigned in greedy");
    }
}
```

### Validation

```bash
cargo test -p lpvm-native --lib regalloc
```
