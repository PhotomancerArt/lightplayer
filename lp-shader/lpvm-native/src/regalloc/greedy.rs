//! Greedy round-robin allocator over [`crate::isa::rv32::abi::ALLOCA_REGS`].

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use super::{Allocation, RegAlloc, VRegInfo};
use crate::isa::rv32::abi::{ALLOCA_REGS, CALLER_SAVED, PhysReg};
use crate::vinst::VInst;

pub struct GreedyAlloc;

impl GreedyAlloc {
    pub const fn new() -> Self {
        Self
    }
}

impl Default for GreedyAlloc {
    fn default() -> Self {
        Self::new()
    }
}

impl RegAlloc for GreedyAlloc {
    fn allocate(&self, vinsts: &[VInst], vreg_info: &VRegInfo) -> Allocation {
        let mut clobbered: BTreeSet<PhysReg> = BTreeSet::new();
        for inst in vinsts {
            if inst.is_call() {
                clobbered.extend(CALLER_SAVED.iter().copied());
            }
        }

        let n = vreg_info.count();
        let mut vreg_to_phys: Vec<Option<PhysReg>> = alloc::vec![None; n];

        let mut next = 0usize;
        for inst in vinsts {
            for v in inst.defs() {
                let i = v.0 as usize;
                if i >= vreg_to_phys.len() {
                    continue;
                }
                if vreg_to_phys[i].is_none() {
                    let phys = ALLOCA_REGS[next % ALLOCA_REGS.len()];
                    next += 1;
                    vreg_to_phys[i] = Some(phys);
                }
            }
        }

        Allocation {
            vreg_to_phys,
            clobbered,
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::String;

    use super::*;
    use crate::types::NativeType;
    use crate::vinst::SymbolRef;

    #[test]
    fn call_clobbers_all_caller_saved() {
        let vinsts = alloc::vec![VInst::Call {
            target: SymbolRef {
                name: String::from("f"),
            },
            args: Vec::new(),
            rets: Vec::new(),
        }];
        let info = VRegInfo { types: Vec::new() };
        let a = GreedyAlloc::new().allocate(&vinsts, &info);
        for reg in CALLER_SAVED {
            assert!(a.clobbered.contains(reg), "x{reg} not clobbered");
        }
    }

    #[test]
    fn callee_saved_not_clobbered_by_call() {
        use crate::isa::rv32::abi::CALLEE_SAVED;
        let vinsts = alloc::vec![VInst::Call {
            target: SymbolRef {
                name: String::from("f"),
            },
            args: Vec::new(),
            rets: Vec::new(),
        }];
        let info = VRegInfo { types: Vec::new() };
        let a = GreedyAlloc::new().allocate(&vinsts, &info);
        for reg in CALLEE_SAVED {
            if !CALLER_SAVED.contains(reg) {
                assert!(
                    !a.clobbered.contains(reg),
                    "callee-saved x{reg} should not be in clobber set"
                );
            }
        }
    }

    #[test]
    fn assigns_defs_round_robin() {
        use lpir::VReg;
        let v0 = VReg(0);
        let v1 = VReg(1);
        let vinsts = alloc::vec![
            VInst::Add32 {
                dst: v0,
                src1: v0,
                src2: v1,
            },
            VInst::Add32 {
                dst: v1,
                src1: v0,
                src2: v1,
            },
        ];
        let info = VRegInfo {
            types: alloc::vec![NativeType::I32, NativeType::I32],
        };
        let a = GreedyAlloc::new().allocate(&vinsts, &info);
        assert!(a.vreg_to_phys[0].is_some());
        assert!(a.vreg_to_phys[1].is_some());
    }
}
