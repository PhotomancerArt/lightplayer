//! Per-call-site vreg / slot remapping for inlined callees.

use alloc::vec::Vec;

use crate::lpir_module::{IrFunction, VMCTX_VREG};
use crate::lpir_op::LpirOp;
use crate::types::{IrType, SlotId, VReg, VRegRange};

const VREG_SENTINEL: VReg = VReg(u32::MAX);

/// One bool per user param (index `i` = param `VReg(i + 1)`).
pub(crate) struct ParamWriteMask {
    pub written: Vec<bool>,
}

pub(crate) fn scan_param_writes(callee: &IrFunction) -> ParamWriteMask {
    let n = callee.param_count as usize;
    let mut written = alloc::vec![false; n];
    for op in &callee.body {
        if let Some(def) = op.def_vreg() {
            debug_assert_ne!(def, VMCTX_VREG, "vmctx should never be defined");
            let i = def.0 as usize;
            if i >= 1 && i <= callee.param_count as usize {
                written[i - 1] = true;
            }
        }
    }
    ParamWriteMask { written }
}

pub(crate) struct Remap {
    pub vreg_table: Vec<VReg>,
    pub param_copies: Vec<LpirOp>,
    pub slot_offset: u32,
}

fn alloc_caller_vreg(caller: &mut IrFunction, ty: IrType) -> VReg {
    let idx = caller.vreg_types.len() as u32;
    caller.vreg_types.push(ty);
    VReg(idx)
}

pub(crate) fn build_remap(
    caller: &mut IrFunction,
    callee: &IrFunction,
    call_args: &[VReg],
    _call_results: &[VReg],
    param_writes: &ParamWriteMask,
) -> Remap {
    let n = callee.vreg_types.len();
    debug_assert_eq!(
        call_args.len(),
        1 + callee.param_count as usize,
        "call args arity"
    );

    let mut vreg_table = alloc::vec![VREG_SENTINEL; n];
    let mut param_copies = Vec::new();

    vreg_table[0] = VMCTX_VREG;

    for i in 1..=callee.param_count as usize {
        let idx = i;
        if !param_writes.written[i - 1] {
            vreg_table[idx] = call_args[i];
        } else {
            let ty = callee.vreg_types[idx];
            let dst = alloc_caller_vreg(caller, ty);
            vreg_table[idx] = dst;
            param_copies.push(LpirOp::Copy {
                dst,
                src: call_args[i],
            });
        }
    }

    for idx in (callee.param_count as usize + 1)..n {
        let ty = callee.vreg_types[idx];
        vreg_table[idx] = alloc_caller_vreg(caller, ty);
    }

    debug_assert!(!vreg_table.iter().any(|&v| v == VREG_SENTINEL));

    let slot_offset = caller.slots.len() as u32;
    for s in &callee.slots {
        caller.slots.push(s.clone());
    }

    Remap {
        vreg_table,
        param_copies,
        slot_offset,
    }
}

fn map_vreg(table: &[VReg], v: VReg) -> VReg {
    table[v.0 as usize]
}

fn map_slot(off: u32, s: SlotId) -> SlotId {
    SlotId(s.0 + off)
}

fn remap_vreg_range(
    range: VRegRange,
    remap: &Remap,
    caller_pool: &mut Vec<VReg>,
    callee_pool: &[VReg],
) -> VRegRange {
    let start_idx = range.start as usize;
    let count = range.count as usize;
    let end = start_idx + count;
    let slice = &callee_pool[start_idx..end];
    let start = caller_pool.len() as u32;
    for &v in slice {
        caller_pool.push(map_vreg(&remap.vreg_table, v));
    }
    VRegRange {
        start,
        count: range.count,
    }
}

pub(crate) fn remap_op(
    op: &LpirOp,
    remap: &Remap,
    caller_vreg_pool: &mut Vec<VReg>,
    callee_vreg_pool: &[VReg],
) -> LpirOp {
    let m = |v: VReg| map_vreg(&remap.vreg_table, v);
    let ms = |s: SlotId| map_slot(remap.slot_offset, s);

    match op {
        LpirOp::Fadd { dst, lhs, rhs } => LpirOp::Fadd {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::Fsub { dst, lhs, rhs } => LpirOp::Fsub {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::Fmul { dst, lhs, rhs } => LpirOp::Fmul {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::Fdiv { dst, lhs, rhs } => LpirOp::Fdiv {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::Fneg { dst, src } => LpirOp::Fneg {
            dst: m(*dst),
            src: m(*src),
        },
        LpirOp::Fabs { dst, src } => LpirOp::Fabs {
            dst: m(*dst),
            src: m(*src),
        },
        LpirOp::Fsqrt { dst, src } => LpirOp::Fsqrt {
            dst: m(*dst),
            src: m(*src),
        },
        LpirOp::Fmin { dst, lhs, rhs } => LpirOp::Fmin {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::Fmax { dst, lhs, rhs } => LpirOp::Fmax {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::Ffloor { dst, src } => LpirOp::Ffloor {
            dst: m(*dst),
            src: m(*src),
        },
        LpirOp::Fceil { dst, src } => LpirOp::Fceil {
            dst: m(*dst),
            src: m(*src),
        },
        LpirOp::Ftrunc { dst, src } => LpirOp::Ftrunc {
            dst: m(*dst),
            src: m(*src),
        },
        LpirOp::Fnearest { dst, src } => LpirOp::Fnearest {
            dst: m(*dst),
            src: m(*src),
        },
        LpirOp::Iadd { dst, lhs, rhs } => LpirOp::Iadd {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::Isub { dst, lhs, rhs } => LpirOp::Isub {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::Imul { dst, lhs, rhs } => LpirOp::Imul {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::IdivS { dst, lhs, rhs } => LpirOp::IdivS {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::IdivU { dst, lhs, rhs } => LpirOp::IdivU {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::IremS { dst, lhs, rhs } => LpirOp::IremS {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::IremU { dst, lhs, rhs } => LpirOp::IremU {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::Ineg { dst, src } => LpirOp::Ineg {
            dst: m(*dst),
            src: m(*src),
        },
        LpirOp::Feq { dst, lhs, rhs } => LpirOp::Feq {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::Fne { dst, lhs, rhs } => LpirOp::Fne {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::Flt { dst, lhs, rhs } => LpirOp::Flt {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::Fle { dst, lhs, rhs } => LpirOp::Fle {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::Fgt { dst, lhs, rhs } => LpirOp::Fgt {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::Fge { dst, lhs, rhs } => LpirOp::Fge {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::Ieq { dst, lhs, rhs } => LpirOp::Ieq {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::Ine { dst, lhs, rhs } => LpirOp::Ine {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::IltS { dst, lhs, rhs } => LpirOp::IltS {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::IleS { dst, lhs, rhs } => LpirOp::IleS {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::IgtS { dst, lhs, rhs } => LpirOp::IgtS {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::IgeS { dst, lhs, rhs } => LpirOp::IgeS {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::IltU { dst, lhs, rhs } => LpirOp::IltU {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::IleU { dst, lhs, rhs } => LpirOp::IleU {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::IgtU { dst, lhs, rhs } => LpirOp::IgtU {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::IgeU { dst, lhs, rhs } => LpirOp::IgeU {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::Iand { dst, lhs, rhs } => LpirOp::Iand {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::Ior { dst, lhs, rhs } => LpirOp::Ior {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::Ixor { dst, lhs, rhs } => LpirOp::Ixor {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::Ibnot { dst, src } => LpirOp::Ibnot {
            dst: m(*dst),
            src: m(*src),
        },
        LpirOp::Ishl { dst, lhs, rhs } => LpirOp::Ishl {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::IshrS { dst, lhs, rhs } => LpirOp::IshrS {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::IshrU { dst, lhs, rhs } => LpirOp::IshrU {
            dst: m(*dst),
            lhs: m(*lhs),
            rhs: m(*rhs),
        },
        LpirOp::FconstF32 { dst, value } => LpirOp::FconstF32 {
            dst: m(*dst),
            value: *value,
        },
        LpirOp::IconstI32 { dst, value } => LpirOp::IconstI32 {
            dst: m(*dst),
            value: *value,
        },
        LpirOp::IaddImm { dst, src, imm } => LpirOp::IaddImm {
            dst: m(*dst),
            src: m(*src),
            imm: *imm,
        },
        LpirOp::IsubImm { dst, src, imm } => LpirOp::IsubImm {
            dst: m(*dst),
            src: m(*src),
            imm: *imm,
        },
        LpirOp::ImulImm { dst, src, imm } => LpirOp::ImulImm {
            dst: m(*dst),
            src: m(*src),
            imm: *imm,
        },
        LpirOp::IshlImm { dst, src, imm } => LpirOp::IshlImm {
            dst: m(*dst),
            src: m(*src),
            imm: *imm,
        },
        LpirOp::IshrSImm { dst, src, imm } => LpirOp::IshrSImm {
            dst: m(*dst),
            src: m(*src),
            imm: *imm,
        },
        LpirOp::IshrUImm { dst, src, imm } => LpirOp::IshrUImm {
            dst: m(*dst),
            src: m(*src),
            imm: *imm,
        },
        LpirOp::IeqImm { dst, src, imm } => LpirOp::IeqImm {
            dst: m(*dst),
            src: m(*src),
            imm: *imm,
        },
        LpirOp::FtoiSatS { dst, src } => LpirOp::FtoiSatS {
            dst: m(*dst),
            src: m(*src),
        },
        LpirOp::FtoiSatU { dst, src } => LpirOp::FtoiSatU {
            dst: m(*dst),
            src: m(*src),
        },
        LpirOp::ItofS { dst, src } => LpirOp::ItofS {
            dst: m(*dst),
            src: m(*src),
        },
        LpirOp::ItofU { dst, src } => LpirOp::ItofU {
            dst: m(*dst),
            src: m(*src),
        },
        LpirOp::FfromI32Bits { dst, src } => LpirOp::FfromI32Bits {
            dst: m(*dst),
            src: m(*src),
        },
        LpirOp::FtoUnorm16 { dst, src } => LpirOp::FtoUnorm16 {
            dst: m(*dst),
            src: m(*src),
        },
        LpirOp::FtoUnorm8 { dst, src } => LpirOp::FtoUnorm8 {
            dst: m(*dst),
            src: m(*src),
        },
        LpirOp::Unorm16toF { dst, src } => LpirOp::Unorm16toF {
            dst: m(*dst),
            src: m(*src),
        },
        LpirOp::Unorm8toF { dst, src } => LpirOp::Unorm8toF {
            dst: m(*dst),
            src: m(*src),
        },
        LpirOp::Select {
            dst,
            cond,
            if_true,
            if_false,
        } => LpirOp::Select {
            dst: m(*dst),
            cond: m(*cond),
            if_true: m(*if_true),
            if_false: m(*if_false),
        },
        LpirOp::Copy { dst, src } => LpirOp::Copy {
            dst: m(*dst),
            src: m(*src),
        },
        LpirOp::SlotAddr { dst, slot } => LpirOp::SlotAddr {
            dst: m(*dst),
            slot: ms(*slot),
        },
        LpirOp::Load { dst, base, offset } => LpirOp::Load {
            dst: m(*dst),
            base: m(*base),
            offset: *offset,
        },
        LpirOp::Store {
            base,
            offset,
            value,
        } => LpirOp::Store {
            base: m(*base),
            offset: *offset,
            value: m(*value),
        },
        LpirOp::Store8 {
            base,
            offset,
            value,
        } => LpirOp::Store8 {
            base: m(*base),
            offset: *offset,
            value: m(*value),
        },
        LpirOp::Store16 {
            base,
            offset,
            value,
        } => LpirOp::Store16 {
            base: m(*base),
            offset: *offset,
            value: m(*value),
        },
        LpirOp::Load8U { dst, base, offset } => LpirOp::Load8U {
            dst: m(*dst),
            base: m(*base),
            offset: *offset,
        },
        LpirOp::Load8S { dst, base, offset } => LpirOp::Load8S {
            dst: m(*dst),
            base: m(*base),
            offset: *offset,
        },
        LpirOp::Load16U { dst, base, offset } => LpirOp::Load16U {
            dst: m(*dst),
            base: m(*base),
            offset: *offset,
        },
        LpirOp::Load16S { dst, base, offset } => LpirOp::Load16S {
            dst: m(*dst),
            base: m(*base),
            offset: *offset,
        },
        LpirOp::Memcpy {
            dst_addr,
            src_addr,
            size,
        } => LpirOp::Memcpy {
            dst_addr: m(*dst_addr),
            src_addr: m(*src_addr),
            size: *size,
        },
        LpirOp::IfStart {
            cond,
            else_offset: _,
            end_offset: _,
        } => LpirOp::IfStart {
            cond: m(*cond),
            else_offset: 0,
            end_offset: 0,
        },
        LpirOp::Else => LpirOp::Else,
        LpirOp::Continuing => LpirOp::Continuing,
        LpirOp::LoopStart {
            continuing_offset: _,
            end_offset: _,
        } => LpirOp::LoopStart {
            continuing_offset: 0,
            end_offset: 0,
        },
        LpirOp::SwitchStart {
            selector,
            end_offset: _,
        } => LpirOp::SwitchStart {
            selector: m(*selector),
            end_offset: 0,
        },
        LpirOp::CaseStart {
            value,
            end_offset: _,
        } => LpirOp::CaseStart {
            value: *value,
            end_offset: 0,
        },
        LpirOp::DefaultStart { end_offset: _ } => LpirOp::DefaultStart { end_offset: 0 },
        LpirOp::End => LpirOp::End,
        LpirOp::Block { end_offset: _ } => LpirOp::Block { end_offset: 0 },
        LpirOp::Break => LpirOp::Break,
        LpirOp::Continue => LpirOp::Continue,
        LpirOp::BrIfNot { cond } => LpirOp::BrIfNot { cond: m(*cond) },
        LpirOp::ExitBlock => LpirOp::ExitBlock,
        LpirOp::Call {
            callee,
            args,
            results,
        } => {
            let callee = *callee;
            let args = remap_vreg_range(*args, remap, caller_vreg_pool, callee_vreg_pool);
            let results = remap_vreg_range(*results, remap, caller_vreg_pool, callee_vreg_pool);
            LpirOp::Call {
                callee,
                args,
                results,
            }
        }
        LpirOp::Return { .. } => op.clone(),
    }
}
