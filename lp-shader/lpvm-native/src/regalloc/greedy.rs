//! Greedy allocator: pin parameters to a0–a7, assign defs to [`crate::isa::rv32::abi::ALLOCA_REGS`].

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use lpir::IrFunction;

use super::{Allocation, RegAlloc};
use crate::error::NativeError;
use crate::isa::rv32::abi::{ALLOCA_REGS, ARG_REGS, CALLER_SAVED, PhysReg};
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
    fn allocate(&self, func: &IrFunction, vinsts: &[VInst]) -> Result<Allocation, NativeError> {
        let n = func.vreg_types.len();
        let slots = func.total_param_slots() as usize;
        if slots > ARG_REGS.len() {
            return Err(NativeError::TooManyArgs(slots));
        }

        let mut vreg_to_phys: Vec<Option<PhysReg>> = alloc::vec![None; n];
        for i in 0..slots {
            if i < n {
                vreg_to_phys[i] = Some(ARG_REGS[i]);
            }
        }

        let mut next_alloca = 0usize;
        for inst in vinsts {
            for v in inst.defs() {
                let vi = v.0 as usize;
                if vi >= n {
                    continue;
                }
                if vreg_to_phys[vi].is_none() {
                    if next_alloca >= ALLOCA_REGS.len() {
                        return Err(NativeError::TooManyVRegs {
                            count: next_alloca + 1,
                            max: ALLOCA_REGS.len(),
                        });
                    }
                    vreg_to_phys[vi] = Some(ALLOCA_REGS[next_alloca]);
                    next_alloca += 1;
                }
            }
        }

        for inst in vinsts {
            for v in inst.uses() {
                let vi = v.0 as usize;
                if vi < n && vreg_to_phys[vi].is_none() {
                    return Err(NativeError::UnassignedVReg(v.0));
                }
            }
        }

        let mut clobbered: BTreeSet<PhysReg> = BTreeSet::new();
        for inst in vinsts {
            if inst.is_call() {
                clobbered.extend(CALLER_SAVED.iter().copied());
            }
        }

        Ok(Allocation {
            vreg_to_phys,
            clobbered,
        })
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::vec;

    use lpir::VReg;

    use super::*;
    use crate::vinst::SymbolRef;

    #[test]
    fn call_clobbers_all_caller_saved() {
        let f = IrFunction {
            name: String::from("f"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 0,
            return_types: vec![],
            vreg_types: vec![],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        };
        let vinsts = alloc::vec![VInst::Call {
            target: SymbolRef {
                name: String::from("g"),
            },
            args: Vec::new(),
            rets: Vec::new(),
        }];
        let a = GreedyAlloc::new().allocate(&f, &vinsts).expect("alloc");
        for reg in CALLER_SAVED {
            assert!(a.clobbered.contains(reg), "x{reg} not clobbered");
        }
    }

    #[test]
    fn callee_saved_not_clobbered_by_call() {
        use crate::isa::rv32::abi::CALLEE_SAVED;
        let f = IrFunction {
            name: String::from("f"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 0,
            return_types: vec![],
            vreg_types: vec![],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        };
        let vinsts = alloc::vec![VInst::Call {
            target: SymbolRef {
                name: String::from("g"),
            },
            args: Vec::new(),
            rets: Vec::new(),
        }];
        let a = GreedyAlloc::new().allocate(&f, &vinsts).expect("alloc");
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
        let f = IrFunction {
            name: String::from("f"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 0,
            return_types: vec![],
            vreg_types: vec![lpir::IrType::I32; 4],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        };
        let vinsts = alloc::vec![
            VInst::IConst32 {
                dst: VReg(1),
                val: 1,
            },
            VInst::IConst32 {
                dst: VReg(2),
                val: 2,
            },
            VInst::Add32 {
                dst: VReg(3),
                src1: VReg(1),
                src2: VReg(2),
            },
        ];
        let a = GreedyAlloc::new().allocate(&f, &vinsts).expect("alloc");
        assert!(a.vreg_to_phys[1].is_some());
        assert!(a.vreg_to_phys[2].is_some());
        assert!(a.vreg_to_phys[3].is_some());
    }
}
