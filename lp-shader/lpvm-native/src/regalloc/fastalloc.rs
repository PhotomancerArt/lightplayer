//! Backward-walk fast register allocation for straight-line [`VInst`] sequences (M2).
//!
//! Produces [`FastAllocation`] plus a shadow [`Allocation`] for frame layout and metadata.
//! Control flow (`Label`, `Br`, `BrIf`) is rejected until M3.

use alloc::vec::Vec;

use lpir::IrFunction;

use super::{
    Allocation, Edit, EditPos, FastAllocation, Location, OperandHome, PhysReg, collect_iconst_defs,
};
use crate::abi::classify::ArgLoc;
use crate::abi::{FuncAbi, PReg, PregSet, RegClass};
use crate::error::NativeError;
use crate::isa::rv32::abi::{ARG_REGS, RET_REGS};
use crate::vinst::{VInst, VReg};

fn abi2_int_preg_to_phys(p: PReg) -> Result<PhysReg, NativeError> {
    match p.class {
        RegClass::Int => Ok(p.hw),
        RegClass::Float => Err(NativeError::UnassignedVReg(p.hw as u32)),
    }
}

fn sorted_allocatable_ints(set: crate::abi::PregSet) -> Vec<PhysReg> {
    let mut v: Vec<PhysReg> = set
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

fn arg_hw_for_index(i: usize, callee_uses_sret: bool) -> PhysReg {
    if callee_uses_sret {
        ARG_REGS[i + 1].hw
    } else {
        ARG_REGS[i].hw
    }
}

/// First vinst that fastalloc cannot handle (control-flow lowering).
fn first_unsupported_control_flow(
    vinsts: &[VInst],
) -> Option<(usize, &'static str, Option<u32>)> {
    for (i, v) in vinsts.iter().enumerate() {
        if matches!(v, VInst::Label(..) | VInst::Br { .. } | VInst::BrIf { .. }) {
            return Some((i, v.mnemonic(), v.src_op()));
        }
    }
    None
}

/// Backward-walk allocator (straight-line only).
#[derive(Debug, Default)]
pub struct FastAllocator;

impl FastAllocator {
    pub const fn new() -> Self {
        Self
    }

    /// Allocate and produce both fast operand/edits output and classic [`Allocation`] for frame/prologue.
    pub fn allocate_with_func_abi(
        &self,
        func: &IrFunction,
        vinsts: &[VInst],
        abi: &FuncAbi,
    ) -> Result<(FastAllocation, Allocation), NativeError> {
        if let Some((vinst_index, mnemonic, lpir_op_index)) =
            first_unsupported_control_flow(vinsts)
        {
            return Err(NativeError::FastallocUnsupportedControlFlow {
                ir_function_name: func.name.clone(),
                vinst_index,
                mnemonic,
                lpir_op_index,
            });
        }

        let max_vreg = vinsts
            .iter()
            .flat_map(|v| v.defs().chain(v.uses()))
            .map(|v| v.0 as usize)
            .max()
            .unwrap_or(0);
        let n = func.vreg_types.len().max(max_vreg + 1);
        let slots = func.total_param_slots() as usize;
        let param_locs = abi.param_locs();
        if slots > param_locs.len() {
            return Err(NativeError::TooManyArgs(slots));
        }

        let alloc_list = sorted_allocatable_ints(abi.allocatable());
        let clobber_hw = {
            let mut bits = 0u32;
            for p in abi.call_clobbers().iter() {
                bits |= 1u32 << p.hw;
            }
            bits
        };

        let mut st = WalkState::new(n, collect_iconst_defs(vinsts, n), alloc_list, clobber_hw);
        st.init_params(slots, param_locs, abi)?;

        let n_insts = vinsts.len();
        let mut use_rec: Vec<Vec<OperandHome>> = alloc::vec![Vec::new(); n_insts];
        let mut def_rec: Vec<Vec<OperandHome>> = alloc::vec![Vec::new(); n_insts];

        for pos in (0..n_insts).rev() {
            let inst = &vinsts[pos];
            st.process_instruction(pos, inst, &mut use_rec[pos], &mut def_rec[pos])?;
        }

        st.finish_alloc(func, vinsts, abi, use_rec, def_rec)
    }
}

struct WalkState {
    n: usize,
    iconst_val: Vec<Option<i32>>,
    allocatable: Vec<PhysReg>,
    clobber_hw: u32,
    vreg_home: Vec<Option<PhysReg>>,
    vreg_spill: Vec<Option<u32>>,
    preg_occ: Vec<Option<VReg>>,
    preg_lru: Vec<u32>,
    time: u32,
    edits: Vec<(EditPos, Edit)>,
    next_spill: u32,
    max_call_saves: u32,
}

impl WalkState {
    fn new(
        n: usize,
        iconst_val: Vec<Option<i32>>,
        allocatable: Vec<PhysReg>,
        clobber_hw: u32,
    ) -> Self {
        Self {
            n,
            iconst_val,
            allocatable,
            clobber_hw,
            vreg_home: alloc::vec![None; n],
            vreg_spill: alloc::vec![None; n],
            preg_occ: alloc::vec![None; 32],
            preg_lru: alloc::vec![0; 32],
            time: 1,
            edits: Vec::new(),
            next_spill: 0,
            max_call_saves: 0,
        }
    }

    fn touch(&mut self, p: PhysReg) {
        self.preg_lru[p as usize] = self.time;
        self.time = self.time.wrapping_add(1);
    }

    fn init_params(
        &mut self,
        slots: usize,
        param_locs: &[ArgLoc],
        abi: &FuncAbi,
    ) -> Result<(), NativeError> {
        let alloca_list = sorted_allocatable_ints(abi.allocatable());
        let mut next_alloca = 0usize;
        for i in 0..slots {
            match param_locs[i] {
                ArgLoc::Reg(p) => {
                    let hw = abi2_int_preg_to_phys(p)?;
                    let v = VReg(i as u32);
                    self.assign_vreg_to_preg_noinsert(v, hw)?;
                }
                ArgLoc::Stack { .. } => {
                    let v = VReg(i as u32);
                    let Some(&hw) = alloca_list.get(next_alloca) else {
                        return Err(NativeError::TooManyArgs(slots));
                    };
                    next_alloca += 1;
                    self.assign_vreg_to_preg_noinsert(v, hw)?;
                }
            }
        }
        Ok(())
    }

    fn assign_vreg_to_preg_noinsert(&mut self, v: VReg, p: PhysReg) -> Result<(), NativeError> {
        let vi = v.0 as usize;
        if self.preg_occ[p as usize].is_some() {
            return Err(NativeError::FastallocInternal(
                "param physical register conflict",
            ));
        }
        self.vreg_home[vi] = Some(p);
        self.preg_occ[p as usize] = Some(v);
        self.touch(p);
        Ok(())
    }

    fn alloc_spill(&mut self) -> u32 {
        let s = self.next_spill;
        self.next_spill += 1;
        s
    }

    fn lru_evict_preg(&mut self) -> Result<PhysReg, NativeError> {
        let mut best: Option<(u32, PhysReg)> = None;
        for &p in &self.allocatable {
            if self.preg_occ[p as usize].is_some() {
                let t = self.preg_lru[p as usize];
                match best {
                    None => best = Some((t, p)),
                    Some((bt, _)) if t < bt => best = Some((t, p)),
                    _ => {}
                }
            }
        }
        let (_, p) = best.ok_or_else(|| {
            NativeError::FastallocInternal("no LRU victim (no occupied allocatable)")
        })?;
        Ok(p)
    }

    fn spill_vreg_to_stack(&mut self, v: VReg, pos: usize) -> Result<(), NativeError> {
        let vi = v.0 as usize;
        let Some(p) = self.vreg_home[vi] else {
            return Ok(());
        };
        let slot = if let Some(s) = self.vreg_spill[vi] {
            s
        } else {
            let s = self.alloc_spill();
            self.vreg_spill[vi] = Some(s);
            s
        };
        self.edits.push((
            EditPos::Before(pos),
            Edit::Move {
                from: Location::Reg(p),
                to: Location::Stack(slot),
            },
        ));
        self.preg_occ[p as usize] = None;
        self.vreg_home[vi] = None;
        Ok(())
    }

    fn reload_from_spill(&mut self, v: VReg, into: PhysReg, pos: usize) -> Result<(), NativeError> {
        let vi = v.0 as usize;
        let Some(slot) = self.vreg_spill[vi] else {
            return Err(NativeError::SpilledVReg(v.0));
        };
        if let Some(occ) = self.preg_occ[into as usize] {
            if occ != v {
                self.spill_vreg_to_stack(occ, pos)?;
            }
        }
        self.edits.push((
            EditPos::Before(pos),
            Edit::Move {
                from: Location::Stack(slot),
                to: Location::Reg(into),
            },
        ));
        self.preg_occ[into as usize] = Some(v);
        self.vreg_home[vi] = Some(into);
        self.touch(into);
        Ok(())
    }

    fn ensure_free_preg(&mut self, want: PhysReg, pos: usize) -> Result<(), NativeError> {
        if let Some(ov) = self.preg_occ[want as usize] {
            self.spill_vreg_to_stack(ov, pos)?;
        }
        Ok(())
    }

    fn move_vreg_preg(
        &mut self,
        v: VReg,
        from: PhysReg,
        to: PhysReg,
        pos: usize,
    ) -> Result<(), NativeError> {
        if from == to {
            self.touch(to);
            return Ok(());
        }
        self.ensure_free_preg(to, pos)?;
        self.edits.push((
            EditPos::Before(pos),
            Edit::Move {
                from: Location::Reg(from),
                to: Location::Reg(to),
            },
        ));
        self.preg_occ[from as usize] = None;
        self.preg_occ[to as usize] = Some(v);
        let vi = v.0 as usize;
        self.vreg_home[vi] = Some(to);
        self.touch(to);
        Ok(())
    }

    /// Pin `v` to `want` for an ABI use (args/returns), emitting moves if needed.
    fn pin_vreg_to_preg(&mut self, v: VReg, want: PhysReg, pos: usize) -> Result<(), NativeError> {
        let vi = v.0 as usize;
        if self.iconst_val[vi].is_some() {
            return Ok(());
        }
        if self.vreg_home[vi] == Some(want) {
            self.touch(want);
            return Ok(());
        }
        self.ensure_free_preg(want, pos)?;
        if self.vreg_home[vi].is_none() {
            if self.vreg_spill[vi].is_some() {
                self.reload_from_spill(v, want, pos)?;
            } else {
                return Err(NativeError::UnassignedVReg(v.0));
            }
        } else if let Some(from) = self.vreg_home[vi] {
            self.move_vreg_preg(v, from, want, pos)?;
        }
        Ok(())
    }

    fn ensure_reg_for_use(
        &mut self,
        v: VReg,
        pos: usize,
        home_buf: &mut Vec<OperandHome>,
    ) -> Result<(), NativeError> {
        let vi = v.0 as usize;
        if let Some(k) = self.iconst_val[vi] {
            home_buf.push(OperandHome::Remat(k));
            return Ok(());
        }
        if self.vreg_home[vi].is_none() && self.vreg_spill[vi].is_some() {
            let p = self.find_free_preg()?;
            self.reload_from_spill(v, p, pos)?;
            home_buf.push(OperandHome::Reg(p));
            return Ok(());
        }
        if self.vreg_home[vi].is_none() {
            let p = if let Some(p) = self.find_free_preg_opt() {
                p
            } else {
                let victim_p = self.lru_evict_preg()?;
                let victim_v = self.preg_occ[victim_p as usize]
                    .ok_or(NativeError::FastallocInternal("LRU state inconsistent"))?;
                self.spill_vreg_to_stack(victim_v, pos)?;
                victim_p
            };
            self.preg_occ[p as usize] = Some(v);
            self.vreg_home[vi] = Some(p);
            self.touch(p);
        }
        let p = self.vreg_home[vi].unwrap();
        // Value is in `p`; prefer register home even if a spill slot exists for this vreg.
        home_buf.push(OperandHome::Reg(p));
        self.touch(p);
        Ok(())
    }

    fn find_free_preg_opt(&self) -> Option<PhysReg> {
        for &p in &self.allocatable {
            if self.preg_occ[p as usize].is_none() {
                return Some(p);
            }
        }
        None
    }

    fn find_free_preg(&self) -> Result<PhysReg, NativeError> {
        self.find_free_preg_opt()
            .ok_or(NativeError::FastallocInternal(
                "no free allocatable register",
            ))
    }

    fn ret_use_operand(
        &mut self,
        v: VReg,
        want: PhysReg,
        pos: usize,
    ) -> Result<OperandHome, NativeError> {
        let vi = v.0 as usize;
        if let Some(k) = self.iconst_val[vi] {
            return Ok(OperandHome::Remat(k));
        }
        self.pin_vreg_to_preg(v, want, pos)?;
        Ok(OperandHome::Reg(want))
    }

    fn process_def(
        &mut self,
        pos: usize,
        v: VReg,
        inst: &VInst,
        def_buf: &mut Vec<OperandHome>,
    ) -> Result<(), NativeError> {
        if let VInst::IConst32 { dst, .. } = inst {
            if *dst == v {
                return Ok(());
            }
        }
        let vi = v.0 as usize;
        if let Some(p) = self.vreg_home[vi] {
            if let Some(s) = self.vreg_spill[vi] {
                def_buf.push(OperandHome::Spill(s));
                self.edits.push((
                    EditPos::After(pos),
                    Edit::Move {
                        from: Location::Reg(p),
                        to: Location::Stack(s),
                    },
                ));
            } else {
                def_buf.push(OperandHome::Reg(p));
            }
            self.preg_occ[p as usize] = None;
            self.vreg_home[vi] = None;
        } else if let Some(s) = self.vreg_spill[vi] {
            def_buf.push(OperandHome::Spill(s));
        } else {
            let p = if let Some(p) = self.find_free_preg_opt() {
                p
            } else {
                let victim_p = self.lru_evict_preg()?;
                let victim_v = self.preg_occ[victim_p as usize]
                    .ok_or(NativeError::FastallocInternal("LRU state inconsistent"))?;
                self.spill_vreg_to_stack(victim_v, pos)?;
                victim_p
            };
            def_buf.push(OperandHome::Reg(p));
            self.preg_occ[p as usize] = Some(v);
            self.vreg_home[vi] = Some(p);
            self.touch(p);
        }
        Ok(())
    }

    /// Values in caller-saved registers that must be spilled before the call (adapter semantics).
    fn call_saves(&self, args: &[VReg], callee_uses_sret: bool) -> Vec<(VReg, PhysReg)> {
        let mut seen = 0u32;
        let mut out = Vec::new();
        for vi in 0..self.n {
            let v = VReg(vi as u32);
            let Some(p) = self.vreg_home[vi] else {
                continue;
            };
            if self.clobber_hw & (1u32 << p) == 0 {
                continue;
            }
            let mut skip = false;
            for (i, a) in args.iter().enumerate() {
                if *a != v {
                    continue;
                }
                let cap = if callee_uses_sret {
                    ARG_REGS.len() - 1
                } else {
                    ARG_REGS.len()
                };
                if i < cap && arg_hw_for_index(i, callee_uses_sret) == p {
                    skip = true;
                    break;
                }
            }
            if skip {
                continue;
            }
            let bit = 1u32 << p;
            if seen & bit == 0 {
                seen |= bit;
                out.push((v, p));
            }
        }
        out.sort_by_key(|(v, _)| v.0);
        out
    }

    fn process_instruction(
        &mut self,
        pos: usize,
        inst: &VInst,
        use_buf: &mut Vec<OperandHome>,
        def_buf: &mut Vec<OperandHome>,
    ) -> Result<(), NativeError> {
        match inst {
            VInst::Call {
                args,
                rets,
                callee_uses_sret,
                ..
            } => {
                for r in rets {
                    let vi = r.0 as usize;
                    if let Some(p) = self.vreg_home[vi] {
                        self.preg_occ[p as usize] = None;
                        self.vreg_home[vi] = None;
                    }
                }

                let saved = self.call_saves(args.as_slice(), *callee_uses_sret);
                let slot_base = self.next_spill;
                self.next_spill += saved.len() as u32;
                self.max_call_saves = self.max_call_saves.max(saved.len() as u32);

                for (i, (_, preg)) in saved.iter().enumerate() {
                    self.edits.push((
                        EditPos::Before(pos),
                        Edit::Move {
                            from: Location::Reg(*preg),
                            to: Location::Stack(slot_base + i as u32),
                        },
                    ));
                }

                for (v, preg) in &saved {
                    self.preg_occ[*preg as usize] = None;
                    self.vreg_home[v.0 as usize] = None;
                }

                let mut ret_preg_mask = 0u32;
                for (i, _) in rets.iter().enumerate() {
                    if i < RET_REGS.len() {
                        ret_preg_mask |= 1u32 << RET_REGS[i].hw;
                    }
                }

                for (i, (_, preg)) in saved.iter().enumerate().rev() {
                    if ret_preg_mask & (1u32 << *preg) != 0 {
                        continue;
                    }
                    self.edits.push((
                        EditPos::After(pos),
                        Edit::Move {
                            from: Location::Stack(slot_base + i as u32),
                            to: Location::Reg(*preg),
                        },
                    ));
                }

                for (v, preg) in &saved {
                    if ret_preg_mask & (1u32 << *preg) != 0 {
                        continue;
                    }
                    self.vreg_home[v.0 as usize] = Some(*preg);
                    self.preg_occ[*preg as usize] = Some(*v);
                    self.touch(*preg);
                }

                let reg_cap = if *callee_uses_sret {
                    ARG_REGS.len() - 1
                } else {
                    ARG_REGS.len()
                };
                for idx in (0..args.len()).rev() {
                    let a = args[idx];
                    if idx < reg_cap {
                        let want = arg_hw_for_index(idx, *callee_uses_sret);
                        self.pin_vreg_to_preg(a, want, pos)?;
                        use_buf.push(OperandHome::Reg(want));
                    } else {
                        self.ensure_reg_for_use(a, pos, use_buf)?;
                    }
                }
                use_buf.reverse();

                for (ri, r) in rets.iter().enumerate() {
                    if ri >= RET_REGS.len() {
                        return Err(NativeError::TooManyReturns(ri + 1));
                    }
                    let want = RET_REGS[ri].hw;
                    self.ensure_free_preg(want, pos)?;
                    self.preg_occ[want as usize] = Some(*r);
                    self.vreg_home[r.0 as usize] = Some(want);
                    self.touch(want);
                    def_buf.push(OperandHome::Reg(want));
                }
            }
            VInst::Ret { vals, .. } => {
                for idx in (0..vals.len()).rev() {
                    if idx >= RET_REGS.len() {
                        return Err(NativeError::TooManyReturns(idx + 1));
                    }
                    let v = vals[idx];
                    let want = RET_REGS[idx].hw;
                    let h = self.ret_use_operand(v, want, pos)?;
                    use_buf.push(h);
                }
                use_buf.reverse();
            }
            _ => {
                for d in inst.defs() {
                    self.process_def(pos, d, inst, def_buf)?;
                }
                for u in inst.uses().collect::<Vec<_>>().into_iter().rev() {
                    self.ensure_reg_for_use(u, pos, use_buf)?;
                }
                use_buf.reverse();
            }
        }
        Ok(())
    }

    fn finish_alloc(
        self,
        func: &IrFunction,
        vinsts: &[VInst],
        abi: &FuncAbi,
        use_rec: Vec<Vec<OperandHome>>,
        def_rec: Vec<Vec<OperandHome>>,
    ) -> Result<(FastAllocation, Allocation), NativeError> {
        let mut operand_homes: Vec<OperandHome> = Vec::new();
        let mut operand_base: Vec<usize> = Vec::with_capacity(vinsts.len());

        for i in 0..vinsts.len() {
            operand_base.push(operand_homes.len());
            for h in &use_rec[i] {
                operand_homes.push(*h);
            }
            for h in &def_rec[i] {
                operand_homes.push(*h);
            }
        }

        let incoming_stack_params = {
            let slots = func.total_param_slots() as usize;
            let param_locs = abi.param_locs();
            let alloca_list = sorted_allocatable_ints(abi.allocatable());
            let mut next_alloca = 0usize;
            let mut v = Vec::new();
            for i in 0..slots {
                if let ArgLoc::Stack { offset, .. } = param_locs[i] {
                    let vr = VReg(i as u32);
                    let Some(&_) = alloca_list.get(next_alloca) else {
                        return Err(NativeError::TooManyArgs(slots));
                    };
                    next_alloca += 1;
                    v.push((vr, offset));
                }
            }
            v
        };

        let mut spill_slots: Vec<VReg> = Vec::new();
        for vi in 0..self.n {
            if self.vreg_spill[vi].is_some() {
                spill_slots.push(VReg(vi as u32));
            }
        }
        spill_slots.sort_by_key(|v| self.vreg_spill[v.0 as usize]);

        let mut rematerial_iconst = alloc::vec![None; self.n];
        for vi in 0..self.n {
            if self.iconst_val[vi].is_some()
                && self.vreg_spill[vi].is_none()
                && self.vreg_home[vi].is_none()
            {
                rematerial_iconst[vi] = self.iconst_val[vi];
            }
        }

        let alloc = Allocation {
            vreg_to_phys: self.vreg_home.clone(),
            clobbered: clobber_set_from_abi(abi),
            spill_slots,
            rematerial_iconst,
            incoming_stack_params,
        };

        let fast = FastAllocation {
            operand_homes,
            operand_base,
            edits: self.edits,
            spill_slot_count: alloc.spill_count(),
            incoming_stack_params: alloc.incoming_stack_params.clone(),
            max_call_preserve_slots: self.max_call_saves,
        };

        Ok((fast, alloc))
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::vec;

    use lpir::{IrFunction, IrType, VReg};

    use super::*;
    use crate::error::NativeError;
    use crate::isa::rv32::abi::func_abi_rv32;
    use crate::regalloc::Allocation;

    fn simple_func(nv: usize) -> IrFunction {
        IrFunction {
            name: String::from("f"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 0,
            return_types: vec![],
            vreg_types: vec![IrType::I32; nv],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        }
    }

    #[test]
    fn straight_line_iconst_add_allocates() {
        let f = simple_func(4);
        let vinsts = vec![
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
        let sig = lps_shared::LpsFnSig {
            name: String::from("f"),
            return_type: lps_shared::LpsType::Void,
            parameters: vec![],
        };
        let abi = func_abi_rv32(&sig, 1);
        let (fast, alloc) = FastAllocator::new()
            .allocate_with_func_abi(&f, &vinsts, &abi)
            .expect("alloc");
        assert_eq!(fast.operand_base.len(), vinsts.len());
        let last = vinsts.len() - 1;
        assert_eq!(
            fast.operand_homes.len(),
            fast.operand_base[last] + use_def_operand_count(&vinsts[last], &alloc)
        );
        assert!(alloc.rematerial_iconst32(VReg(1)).is_some());
    }

    #[test]
    fn fastalloc_rejects_control_flow() {
        let f = simple_func(2);
        let vinsts = vec![VInst::Br {
            target: 0,
            src_op: None,
        }];
        let sig = lps_shared::LpsFnSig {
            name: String::from("f"),
            return_type: lps_shared::LpsType::Void,
            parameters: vec![],
        };
        let abi = func_abi_rv32(&sig, 1);
        let e = FastAllocator::new()
            .allocate_with_func_abi(&f, &vinsts, &abi)
            .expect_err("expected CF error");
        match e {
            NativeError::FastallocUnsupportedControlFlow {
                ir_function_name,
                vinst_index,
                mnemonic,
                lpir_op_index,
            } => {
                assert_eq!(ir_function_name, "f");
                assert_eq!(vinst_index, 0);
                assert_eq!(mnemonic, "Br");
                assert!(lpir_op_index.is_none());
            }
            _ => panic!("expected FastallocUnsupportedControlFlow, got {e:?}"),
        }
    }

    fn use_def_operand_count(inst: &VInst, alloc: &Allocation) -> usize {
        let mut n = inst.uses().count();
        for d in inst.defs() {
            if let VInst::IConst32 { dst, .. } = inst {
                if *dst == d && alloc.rematerial_iconst32(d).is_some() {
                    continue;
                }
            }
            n += 1;
        }
        n
    }
}
