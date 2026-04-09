//! Greedy allocator: pin parameters to a0–a7, assign defs to [`crate::isa::rv32::abi::ALLOCA_REGS`].

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use lpir::IrFunction;

use super::{Allocation, RegAlloc};
use crate::error::NativeError;
use crate::isa::rv32::abi::{ALLOCA_REGS, ARG_REGS, CALLER_SAVED, PhysReg};
use crate::vinst::{VInst, VReg};

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
    fn allocate(
        &self,
        func: &IrFunction,
        vinsts: &[VInst],
        arg_reg_offset: usize,
    ) -> Result<Allocation, NativeError> {
        // First, find the maximum vreg index used in the function
        // This includes vregs from vreg_pool (e.g., used in Return)
        let max_vreg = vinsts
            .iter()
            .flat_map(|v| v.defs().chain(v.uses()))
            .map(|v| v.0 as usize)
            .max()
            .unwrap_or(0);

        // Total vregs = max index + 1 (must cover all used vreg indices)
        let n = (func.vreg_types.len()).max(max_vreg + 1);

        let slots = func.total_param_slots() as usize;
        // For sret, arg_reg_offset=1, so we need slots + 1 <= ARG_REGS.len()
        if slots + arg_reg_offset > ARG_REGS.len() {
            return Err(NativeError::TooManyArgs(slots));
        }

        let mut vreg_to_phys: Vec<Option<PhysReg>> = alloc::vec![None; n];
        let mut spill_slots: Vec<VReg> = Vec::new();

        // Assign params to arg regs with offset (sret shifts params to a1+)
        for i in 0..slots.min(n) {
            vreg_to_phys[i] = Some(ARG_REGS[arg_reg_offset + i]);
        }

        // Collect all vregs that need assignment (defs + uses that aren't defs)
        // This handles pool vregs like those in Ret { vals: ... } that are used
        // but may not be defined as VInst defs (they're defined by LPIR body ops)
        let mut needs_assignment: alloc::collections::BTreeSet<VReg> =
            alloc::collections::BTreeSet::new();

        // Track which vregs are used in Return - these MUST have registers
        // (cannot be spilled) because they need to be in RET_REGS for the ABI
        let mut ret_uses: alloc::collections::BTreeSet<VReg> =
            alloc::collections::BTreeSet::new();

        for inst in vinsts {
            for v in inst.defs() {
                needs_assignment.insert(v);
            }
            for v in inst.uses() {
                needs_assignment.insert(v);
            }
            // Identify return values - they must be in registers
            if let VInst::Ret { vals, .. } = inst {
                for v in vals {
                    ret_uses.insert(*v);
                }
            }
        }

        // Assign to allocatable regs, spill when exhausted
        // First assign return values to ensure they get registers
        let mut next_alloca = 0usize;


        for v in &ret_uses {
            let vi = v.0 as usize;
            if vi >= n || vreg_to_phys[vi].is_some() {
                continue;
            }
            if next_alloca < ALLOCA_REGS.len() {
                vreg_to_phys[vi] = Some(ALLOCA_REGS[next_alloca]);
                next_alloca += 1;
            } else {
                // Return value couldn't get a register - this is an error
                // because we can't spill return values (ABI requires them in registers)
                return Err(NativeError::TooManyReturns(ret_uses.len()));
            }
        }

        // Then assign all other vregs
        for v in needs_assignment {
            let vi = v.0 as usize;
            if vi >= n || vreg_to_phys[vi].is_some() {
                continue;
            }

            if next_alloca < ALLOCA_REGS.len() {
                vreg_to_phys[vi] = Some(ALLOCA_REGS[next_alloca]);
                next_alloca += 1;
            } else {
                // Spill: no register, assign to spill slot
                spill_slots.push(v);
                // vreg_to_phys[vi] remains None to indicate spilled
            }
        }

        // Verify all used vregs have allocation or spill
        for inst in vinsts {
            for v in inst.uses() {
                let vi = v.0 as usize;
                if vi >= n {
                    return Err(NativeError::UnassignedVReg(v.0));
                }
                if vreg_to_phys[vi].is_none() && !spill_slots.contains(&v) {
                    return Err(NativeError::UnassignedVReg(v.0));
                }
            }
        }


        // Collect clobbers (unchanged)
        let mut clobbered: BTreeSet<PhysReg> = BTreeSet::new();
        for inst in vinsts {
            if inst.is_call() {
                clobbered.extend(CALLER_SAVED.iter().copied());
            }
        }

        Ok(Allocation {
            vreg_to_phys,
            clobbered,
            spill_slots,
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
            src_op: None,
        }];
        let a = GreedyAlloc::new().allocate(&f, &vinsts, 0).expect("alloc");
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
            src_op: None,
        }];
        let a = GreedyAlloc::new().allocate(&f, &vinsts, 0).expect("alloc");
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
                src_op: None,
            },
            VInst::IConst32 {
                dst: VReg(2),
                val: 2,
                src_op: None,
            },
            VInst::Add32 {
                dst: VReg(3),
                src1: VReg(1),
                src2: VReg(2),
                src_op: None,
            },
        ];
        let a = GreedyAlloc::new().allocate(&f, &vinsts, 0).expect("alloc");
        assert!(a.vreg_to_phys[1].is_some());
        assert!(a.vreg_to_phys[2].is_some());
        assert!(a.vreg_to_phys[3].is_some());
    }

    // Spill tests
    fn func_with_n_vregs(n: usize) -> IrFunction {
        IrFunction {
            name: String::from("spill_test"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 0,
            return_types: vec![],
            vreg_types: alloc::vec![lpir::IrType::I32; n],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        }
    }

    fn many_iconst_vinsts(count: usize) -> Vec<VInst> {
        (1..=count)
            .map(|i| VInst::IConst32 {
                dst: VReg(i as u32),
                val: i as i32,
                src_op: None,
            })
            .collect()
    }

    #[test]
    fn greedy_spills_when_registers_exhausted() {
        // Create more vregs than available ALLOCA_REGS
        // ALLOCA_REGS has 16 registers (s0-s11 + t3-t6), but s0 is reserved as frame pointer
        // So effectively fewer for allocation - we use 30 vregs to ensure spilling
        let f = func_with_n_vregs(30);
        let vinsts = many_iconst_vinsts(29); // vregs 1-29
        let alloc = GreedyAlloc::new().allocate(&f, &vinsts, 0).expect("alloc");
        assert!(
            alloc.spill_count() > 0,
            "expected spills when registers exhausted, got {} spills",
            alloc.spill_count()
        );
    }

    #[test]
    fn spilled_vreg_has_no_phys_reg() {
        let f = func_with_n_vregs(30);
        let vinsts = many_iconst_vinsts(29);
        let alloc = GreedyAlloc::new().allocate(&f, &vinsts, 0).expect("alloc");
        for spilled in &alloc.spill_slots {
            let vi = spilled.0 as usize;
            assert!(
                alloc.vreg_to_phys[vi].is_none(),
                "spilled vreg {} should have no physical register",
                spilled.0
            );
        }
    }

    #[test]
    fn is_spilled_detects_spilled_vregs() {
        let f = func_with_n_vregs(30);
        let vinsts = many_iconst_vinsts(29);
        let alloc = GreedyAlloc::new().allocate(&f, &vinsts, 0).expect("alloc");
        for spilled in &alloc.spill_slots {
            assert!(
                alloc.is_spilled(*spilled),
                "is_spilled should return true for spilled vreg {}",
                spilled.0
            );
        }
    }

    #[test]
    fn spill_count_matches_spill_slots_len() {
        let f = func_with_n_vregs(30);
        let vinsts = many_iconst_vinsts(29);
        let alloc = GreedyAlloc::new().allocate(&f, &vinsts, 0).expect("alloc");
        assert_eq!(alloc.spill_count(), alloc.spill_slots.len() as u32);
    }

    /// Test that vregs from vreg_pool (which may have indices >= vreg_types.len())
    /// are handled correctly. This reproduces the "unassigned vreg" issue when
    /// Return uses vreg_pool vregs.
    #[test]
    fn handles_vreg_pool_return_vregs() {
        // Simulate a function where:
        // - vreg 0 = vmctx
        // - vregs 1-3 = body defined (in vreg_types)
        // - vreg 35 = from vreg_pool (used in Return)
        let f = IrFunction {
            name: String::from("pool_test"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 0,
            return_types: vec![lpir::IrType::I32],
            vreg_types: vec![
                lpir::IrType::Pointer, // 0: vmctx
                lpir::IrType::I32,     // 1: local
                lpir::IrType::I32,     // 2: local
                lpir::IrType::I32,     // 3: local
            ],
            slots: vec![],
            body: vec![],
            // vreg 35 is in the pool but NOT in vreg_types!
            vreg_pool: vec![VReg(35)],
        };

        // Ret uses vreg 35 from the pool
        let vinsts = alloc::vec![VInst::Ret {
            vals: vec![VReg(35)],
            src_op: None,
        }];

        // This should either:
        // 1. Assign vreg 35 to a register/spill, or
        // 2. Error clearly if the design doesn't support this
        let result = GreedyAlloc::new().allocate(&f, &vinsts, 0);

        // Currently this fails with UnassignedVReg(35)
        // The fix should make this pass
        assert!(
            result.is_ok(),
            "allocator should handle vreg_pool vregs, got: {:?}",
            result
        );
    }

    /// Test that mirrors the filetest scenario: many local vars with a Ret
    /// using a vreg from the pool that's beyond the vreg_types range.
    #[test]
    fn handles_26_locals_plus_pool_return() {
        // Simulate 26 local ints (like spill_simple.glsl)
        // vreg 0 = vmctx, vregs 1-26 = locals, vreg 27 = return value from pool
        let mut vreg_types = alloc::vec![lpir::IrType::Pointer]; // 0: vmctx
        for _ in 0..26 {
            vreg_types.push(lpir::IrType::I32); // 1-26: 26 locals
        }

        let f = IrFunction {
            name: String::from("spill_test"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 0,
            return_types: vec![lpir::IrType::I32],
            vreg_types,
            slots: vec![],
            body: vec![],
            // vreg 27 is in the pool - NOT in vreg_types range (which is 0..27)
            vreg_pool: vec![VReg(27)],
        };

        // Create IConst32 for each local, then Ret using vreg 27
        let mut vinsts: Vec<VInst> = (1..=26)
            .map(|i| VInst::IConst32 {
                dst: VReg(i),
                val: i as i32,
                src_op: None,
            })
            .collect();

        // Ret uses vreg 27 (from pool) which is outside 0..27
        vinsts.push(VInst::Ret {
            vals: vec![VReg(27)],
            src_op: None,
        });

        let result = GreedyAlloc::new().allocate(&f, &vinsts, 0);
        assert!(
            result.is_ok(),
            "allocator should handle 26 locals + pool vreg, got: {:?}",
            result
        );
    }
}
