//! Greedy allocator: pin parameters to ABI locations, assign defs to [`crate::abi::FuncAbi`] allocatable int set.

use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use lpir::IrFunction;

use super::{Allocation, PReg, RegAlloc};
use crate::abi::classify::ArgLoc;
use crate::abi::{FuncAbi, PregSet, RegClass};
use crate::error::NativeError;
use crate::isa::rv32::abi::{ARG_REGS, RET_REGS, alloca_base_int, caller_saved_int};
use crate::vinst::{VInst, VReg};

fn abi2_int_preg_to_phys(p: crate::abi::PReg) -> Result<PReg, NativeError> {
    match p.class {
        RegClass::Int => Ok(p.hw),
        RegClass::Float => Err(NativeError::UnassignedVReg(p.hw as u32)),
    }
}

/// Integer [`PReg`]s in `set`, sorted by hardware index (deterministic allocation order).
fn sorted_allocatable_ints(set: crate::abi::PregSet) -> Vec<PReg> {
    let mut v: Vec<PReg> = set
        .iter()
        .filter(|p| p.class == RegClass::Int)
        .map(|p| p.hw)
        .collect();
    v.sort_unstable();
    v
}

fn clobber_set_from_abi(abi: &FuncAbi) -> PregSet {
    abi.call_clobbers()
}

/// Helper to convert caller_saved_int() PregSet to PregSet for tests.
#[cfg(test)]
fn caller_saved_set() -> PregSet {
    caller_saved_int()
}

pub struct GreedyAlloc;

impl GreedyAlloc {
    pub const fn new() -> Self {
        Self
    }

    /// Allocate using [`FuncAbi`] (precolors, allocatable set, Cranelift-matched clobbers).
    ///
    /// Return-value vregs may spill when no allocatable register remains (including sret paths:
    /// [`crate::isa::rv32::emit`] reloads through spill temps when storing to the sret buffer).
    pub fn allocate_with_func_abi(
        &self,
        func: &IrFunction,
        vinsts: &[VInst],
        abi: &FuncAbi,
    ) -> Result<Allocation, NativeError> {
        let max_vreg = vinsts
            .iter()
            .flat_map(|v| v.defs().chain(v.uses()))
            .map(|v| v.0 as usize)
            .max()
            .unwrap_or(0);

        let n = (func.vreg_types.len()).max(max_vreg + 1);
        let slots = func.total_param_slots() as usize;
        let param_locs = abi.param_locs();

        if slots > param_locs.len() {
            return Err(NativeError::TooManyArgs(slots));
        }

        let mut vreg_to_phys: Vec<Option<PReg>> = alloc::vec![None; n];
        let mut spill_slots: Vec<VReg> = Vec::new();
        let mut rematerial_iconst: Vec<Option<i32>> = alloc::vec![None; n];
        let mut incoming_stack_params: Vec<(VReg, i32)> = Vec::new();

        let iconst_defs = super::collect_iconst_defs(vinsts, n);

        let alloca_list = sorted_allocatable_ints(abi.allocatable());
        let mut next_alloca = 0usize;

        for i in 0..slots {
            match param_locs[i] {
                ArgLoc::Reg(p) => {
                    vreg_to_phys[i] = Some(abi2_int_preg_to_phys(p)?);
                }
                ArgLoc::Stack { offset, .. } => {
                    let v = VReg(i as u32);
                    let Some(p) = alloca_list.get(next_alloca).copied() else {
                        return Err(NativeError::TooManyArgs(slots));
                    };
                    vreg_to_phys[i] = Some(p);
                    incoming_stack_params.push((v, offset));
                    next_alloca += 1;
                }
            }
        }

        let mut needs_assignment: BTreeSet<VReg> = BTreeSet::new();
        let mut ret_uses: BTreeSet<VReg> = BTreeSet::new();

        for inst in vinsts {
            for v in inst.defs() {
                needs_assignment.insert(v);
            }
            for v in inst.uses() {
                needs_assignment.insert(v);
            }
            if let VInst::Ret { vals, .. } = inst {
                for v in vals {
                    ret_uses.insert(*v);
                }
            }
        }

        for v in &ret_uses {
            let vi = v.0 as usize;
            if vi >= n || vreg_to_phys[vi].is_some() {
                continue;
            }
            if next_alloca < alloca_list.len() {
                vreg_to_phys[vi] = Some(alloca_list[next_alloca]);
                next_alloca += 1;
            } else if let Some(k) = iconst_defs[vi] {
                rematerial_iconst[vi] = Some(k);
            } else {
                spill_slots.push(*v);
            }
        }

        for v in needs_assignment {
            let vi = v.0 as usize;
            if vi >= n || vreg_to_phys[vi].is_some() {
                continue;
            }

            if next_alloca < alloca_list.len() {
                vreg_to_phys[vi] = Some(alloca_list[next_alloca]);
                next_alloca += 1;
            } else if let Some(k) = iconst_defs[vi] {
                rematerial_iconst[vi] = Some(k);
            } else {
                spill_slots.push(v);
            }
        }

        for inst in vinsts {
            for v in inst.uses() {
                let vi = v.0 as usize;
                if vi >= n {
                    return Err(NativeError::UnassignedVReg(v.0));
                }
                if vreg_to_phys[vi].is_none()
                    && !spill_slots.contains(&v)
                    && rematerial_iconst[vi].is_none()
                {
                    return Err(NativeError::UnassignedVReg(v.0));
                }
            }
        }

        let mut clobbered = PregSet::EMPTY;
        for inst in vinsts {
            if inst.is_call() {
                clobbered = clobbered.union(clobber_set_from_abi(abi));
            }
        }

        Ok(Allocation {
            vreg_to_phys,
            clobbered,
            spill_slots,
            rematerial_iconst,
            incoming_stack_params,
        })
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
        // Build a simple FuncAbi for legacy direct-return (non-sret) allocation
        let return_method = crate::abi::classify::ReturnMethod::Direct {
            locs: RET_REGS[..2]
                .iter()
                .map(|r| crate::abi::classify::ArgLoc::Reg(*r))
                .collect(),
        };
        let allocatable = alloca_base_int(); // s1 is available for direct returns
        let precolors: Vec<(u32, PReg)> = (0..func.total_param_slots() as usize)
            .enumerate()
            .map(|(i, vreg)| (vreg as u32, ARG_REGS[arg_reg_offset + i]))
            .collect();
        let caller_saved = caller_saved_int();
        let callee_saved = crate::isa::rv32::abi::callee_saved_int();

        let abi = FuncAbi::new_raw(
            precolors
                .iter()
                .map(|(_, p)| crate::abi::classify::ArgLoc::Reg(*p))
                .collect(),
            return_method,
            allocatable,
            precolors,
            caller_saved,
            callee_saved,
        );

        self.allocate_with_func_abi(func, vinsts, &abi)
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
        use crate::regalloc::clobber_set_callee_saved;

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
            callee_uses_sret: false,
            src_op: None,
        }];
        let a = GreedyAlloc::new().allocate(&f, &vinsts, 0).expect("alloc");
        for preg in caller_saved_set().iter() {
            let reg = preg.hw;
            assert!(a.clobbered.contains(preg), "x{reg} not clobbered");
        }
        // Verify no callee-saved registers are clobbered
        for preg in clobber_set_callee_saved().iter() {
            let reg = preg.hw;
            if !caller_saved_set().contains(preg) {
                assert!(
                    !a.clobbered.contains(preg),
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

    /// Copies from vmctx (`v0`); not rematerializable, so excess vregs use stack spill slots.
    fn many_mov_from_vmctx(count: usize) -> Vec<VInst> {
        (1..=count)
            .map(|i| VInst::Mov32 {
                dst: VReg(i as u32),
                src: VReg(0),
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
        let vinsts = many_mov_from_vmctx(29); // vregs 1-29
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
        let vinsts = many_mov_from_vmctx(29);
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
        let vinsts = many_mov_from_vmctx(29);
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
        let vinsts = many_mov_from_vmctx(29);
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

    #[test]
    fn allocate_with_func_abi_sret_vmctx_in_a1() {
        use lps_shared::{LpsFnSig, LpsType};

        use crate::isa::rv32::abi::func_abi_rv32;
        use crate::isa::rv32::abi::{A0, A1};

        let sig = LpsFnSig {
            name: String::from("f"),
            return_type: LpsType::Vec4,
            parameters: Vec::new(),
        };
        let f = IrFunction {
            name: String::from("f"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 0,
            return_types: vec![lpir::IrType::F32; 4],
            vreg_types: vec![lpir::IrType::Pointer],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        };
        let vinsts: Vec<VInst> = Vec::new();
        let abi = func_abi_rv32(&sig, f.total_param_slots() as usize);
        let a = GreedyAlloc::new()
            .allocate_with_func_abi(&f, &vinsts, &abi)
            .expect("alloc");
        assert_eq!(a.vreg_to_phys[0], Some(A1.hw), "vmctx uses a1 when sret");
        assert_ne!(a.vreg_to_phys[0], Some(A0.hw));
    }
}
