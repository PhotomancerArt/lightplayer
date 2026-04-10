//! Convert [`Allocation`] into [`FastAllocation`] for the edit-list emitter (M1).

use alloc::vec::Vec;

use lpir::VReg;

use super::{Allocation, Edit, EditPos, FastAllocation, Location, OperandHome, PhysReg};
use crate::vinst::VInst;

/// Parameters for generating call save/restore [`Edit`] entries (mirrors [`crate::isa::rv32::emit::CallSaveLayout`]).
#[derive(Clone, Copy, Debug)]
pub struct CallSaveEditParams {
    pub slot_base: u32,
    pub clobber_hw: u32,
    pub s1_slot: Option<u32>,
    pub caller_is_sret: bool,
}

fn operand_home_for(alloc: &Allocation, v: VReg) -> OperandHome {
    if let Some(k) = alloc.rematerial_iconst32(v) {
        OperandHome::Remat(k)
    } else if let Some(s) = alloc.spill_slot(v) {
        OperandHome::Spill(s)
    } else {
        OperandHome::Reg(
            alloc.vreg_to_phys[v.0 as usize].expect("adapter: vreg must have reg, spill, or remat"),
        )
    }
}

fn regs_saved_for_call(alloc: &Allocation, rets: &[VReg], clobber: u32) -> Vec<(VReg, PhysReg)> {
    let mut seen = 0u32;
    let mut out = Vec::new();
    for (vi, po) in alloc.vreg_to_phys.iter().enumerate() {
        let Some(p) = po else {
            continue;
        };
        let v = VReg(vi as u32);
        if alloc.is_spilled(v) {
            continue;
        }
        if rets.contains(&v) {
            continue;
        }
        if clobber & (1u32 << *p) == 0 {
            continue;
        }

        let bit = 1u32 << *p;
        if seen & bit == 0 {
            seen |= bit;
            out.push((v, *p));
        }
    }
    out.sort_by_key(|(v, _)| v.0);
    out
}

fn phys_homes_of_non_spilled(alloc: &Allocation, vregs: &[VReg]) -> u32 {
    let mut out = 0u32;
    for v in vregs {
        if !alloc.is_spilled(*v) {
            if let Some(p) = alloc.vreg_to_phys.get(v.0 as usize).copied().flatten() {
                out |= 1u32 << p;
            }
        }
    }
    out
}

pub struct AllocationAdapter;

impl AllocationAdapter {
    /// Build [`FastAllocation`] from a classic [`Allocation`], including call clobber
    /// save/restore as explicit [`Edit`] entries.
    pub fn adapt(
        alloc: &Allocation,
        vinsts: &[VInst],
        call_save: Option<CallSaveEditParams>,
    ) -> FastAllocation {
        let mut operand_homes: Vec<OperandHome> = Vec::new();
        let mut operand_base: Vec<usize> = Vec::with_capacity(vinsts.len());

        for inst in vinsts {
            operand_base.push(operand_homes.len());
            for v in inst.uses() {
                operand_homes.push(operand_home_for(alloc, v));
            }
            for d in inst.defs() {
                if let VInst::IConst32 { dst, .. } = inst {
                    if alloc.rematerial_iconst32(*dst).is_some() {
                        continue;
                    }
                }
                operand_homes.push(operand_home_for(alloc, d));
            }
        }

        let mut edits: Vec<(EditPos, Edit)> = Vec::new();
        if let Some(cs) = call_save {
            for (pos, inst) in vinsts.iter().enumerate() {
                if let VInst::Call { rets, .. } = inst {
                    if let Some(s1_slot) = cs.s1_slot {
                        if cs.caller_is_sret {
                            edits.push((
                                EditPos::Before(pos),
                                Edit::Move {
                                    from: Location::Reg(crate::isa::rv32::abi::S1.hw),
                                    to: Location::Stack(s1_slot),
                                },
                            ));
                        }
                    }
                    let saved = regs_saved_for_call(alloc, rets.as_slice(), cs.clobber_hw);
                    for (i, (_, preg)) in saved.iter().enumerate() {
                        let slot = cs.slot_base + i as u32;
                        edits.push((
                            EditPos::Before(pos),
                            Edit::Move {
                                from: Location::Reg(*preg),
                                to: Location::Stack(slot),
                            },
                        ));
                    }
                    let ret_homes = phys_homes_of_non_spilled(alloc, rets.as_slice());
                    for (i, (_, preg)) in saved.iter().enumerate().rev() {
                        if ret_homes & (1u32 << *preg) != 0 {
                            continue;
                        }
                        let slot = cs.slot_base + i as u32;
                        edits.push((
                            EditPos::After(pos),
                            Edit::Move {
                                from: Location::Stack(slot),
                                to: Location::Reg(*preg),
                            },
                        ));
                    }
                    if let Some(s1_slot) = cs.s1_slot {
                        if cs.caller_is_sret {
                            edits.push((
                                EditPos::After(pos),
                                Edit::Move {
                                    from: Location::Stack(s1_slot),
                                    to: Location::Reg(crate::isa::rv32::abi::S1.hw),
                                },
                            ));
                        }
                    }
                }
            }
        }

        FastAllocation {
            operand_homes,
            operand_base,
            edits,
            spill_slot_count: alloc.spill_count(),
            incoming_stack_params: alloc.incoming_stack_params.clone(),
            max_call_preserve_slots: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::vec;

    use lpir::{IrFunction, IrType, VReg};

    use super::*;
    use crate::regalloc::{GreedyAlloc, RegAlloc};
    use crate::vinst::SymbolRef;

    #[test]
    fn adapter_no_call_has_no_edits() {
        let f = IrFunction {
            name: String::from("f"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 0,
            return_types: vec![],
            vreg_types: vec![IrType::I32; 4],
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
            VInst::Add32 {
                dst: VReg(3),
                src1: VReg(1),
                src2: VReg(2),
                src_op: None,
            },
        ];
        let alloc = GreedyAlloc::new().allocate(&f, &vinsts, 0).expect("alloc");
        let fast = AllocationAdapter::adapt(&alloc, &vinsts, None);
        assert!(fast.edits.is_empty());
        assert_eq!(fast.operand_base.len(), 2);
        assert_eq!(
            fast.operand_homes.len(),
            1 + 3 // iconst 1 def + add 2 uses + 1 def
        );
    }

    #[test]
    fn adapter_call_edits_balanced_when_present() {
        let f = IrFunction {
            name: String::from("f"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 0,
            return_types: vec![],
            vreg_types: vec![IrType::Pointer, IrType::I32],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        };
        let vinsts = alloc::vec![
            VInst::Mov32 {
                dst: VReg(1),
                src: VReg(0),
                src_op: None,
            },
            VInst::Call {
                target: SymbolRef {
                    name: String::from("g"),
                },
                args: alloc::vec![VReg(1)],
                rets: alloc::vec![],
                callee_uses_sret: false,
                src_op: None,
            },
        ];
        let alloc = GreedyAlloc::new().allocate(&f, &vinsts, 0).expect("alloc");
        let mut clobber_hw = 0u32;
        for p in crate::isa::rv32::abi::caller_saved_int().iter() {
            clobber_hw |= 1u32 << p.hw;
        }
        let cs = CallSaveEditParams {
            slot_base: alloc.spill_count(),
            clobber_hw,
            s1_slot: None,
            caller_is_sret: false,
        };
        let fast = AllocationAdapter::adapt(&alloc, &vinsts, Some(cs));
        let before = fast
            .edits
            .iter()
            .filter(|(p, _)| matches!(p, EditPos::Before(1)))
            .count();
        let after = fast
            .edits
            .iter()
            .filter(|(p, _)| matches!(p, EditPos::After(1)))
            .count();
        assert_eq!(before, after);
    }
}
