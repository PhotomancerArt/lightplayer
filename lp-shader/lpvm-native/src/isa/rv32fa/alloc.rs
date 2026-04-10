//! Straight-line allocation: [`VInst`](crate::vinst::VInst) → [`PInst`](super::inst::PInst).

use alloc::vec::Vec;
use core::fmt;

use lpir::{IrFunction, VReg};

use super::abi::{self, PhysReg, RET_REGS, SCRATCH};
use super::inst::PInst;
use crate::abi::FuncAbi;
use crate::abi::classify::ArgLoc;
use crate::vinst::{IcmpCond, VInst};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllocError {
    UnsupportedControlFlow,
    UnsupportedCall,
    UnsupportedSret,
    UnsupportedSelect,
    UnsupportedStackParams,
    PoolExhausted,
}

impl fmt::Display for AllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AllocError::UnsupportedControlFlow => write!(f, "branches/jumps not supported"),
            AllocError::UnsupportedCall => write!(f, "calls not supported"),
            AllocError::UnsupportedSret => write!(f, "sret returns not supported"),
            AllocError::UnsupportedSelect => write!(f, "Select32 not supported"),
            AllocError::UnsupportedStackParams => write!(f, "stack parameters not supported"),
            AllocError::PoolExhausted => write!(f, "register pool exhausted"),
        }
    }
}

fn max_vreg_index(vinsts: &[VInst], func: &IrFunction) -> usize {
    let mut m = func.vreg_types.len().max(func.total_param_slots() as usize);
    for inst in vinsts {
        for u in inst.uses() {
            m = m.max(u.0 as usize + 1);
        }
        for d in inst.defs() {
            m = m.max(d.0 as usize + 1);
        }
    }
    m.min(256)
}

fn compute_last_use(vinsts: &[VInst], n_vreg: usize) -> Vec<usize> {
    let mut last = vec![0usize; n_vreg];
    for (i, inst) in vinsts.iter().enumerate() {
        for u in inst.uses() {
            let vi = u.0 as usize;
            if vi < n_vreg {
                last[vi] = i;
            }
        }
        if let VInst::Ret { vals, .. } = inst {
            for v in vals {
                let vi = v.0 as usize;
                if vi < n_vreg {
                    last[vi] = i;
                }
            }
        }
    }
    last
}

pub fn allocate(
    vinsts: &[VInst],
    func_abi: &FuncAbi,
    func: &IrFunction,
) -> Result<Vec<PInst>, AllocError> {
    if func_abi.is_sret() {
        return Err(AllocError::UnsupportedSret);
    }
    for i in 0..func.total_param_slots() as usize {
        if matches!(func_abi.param_loc(i), Some(ArgLoc::Stack { .. })) {
            return Err(AllocError::UnsupportedStackParams);
        }
    }

    for inst in vinsts {
        match inst {
            VInst::Br { .. } | VInst::BrIf { .. } => {
                return Err(AllocError::UnsupportedControlFlow);
            }
            VInst::Select32 { .. } => return Err(AllocError::UnsupportedSelect),
            VInst::Call { .. } => return Err(AllocError::UnsupportedCall),
            _ => {}
        }
    }

    let n = max_vreg_index(vinsts, func);
    let last_use = compute_last_use(vinsts, n);
    let mut is_param = vec![false; n];
    for i in 0..func.total_param_slots() as usize {
        if i < n {
            is_param[i] = true;
        }
    }

    let mut preg: Vec<Option<PhysReg>> = vec![None; n];
    for i in 0..func.total_param_slots() as u32 {
        if let Some(p) = func_abi.precolor_of(i) {
            preg[i as usize] = Some(p.hw);
        }
    }

    let mut free: Vec<PhysReg> = abi::ALLOC_POOL.iter().rev().copied().collect();
    let mut out: Vec<PInst> = Vec::new();
    let alloc_reg = |free: &mut Vec<PhysReg>| -> Result<PhysReg, AllocError> {
        free.pop().ok_or(AllocError::PoolExhausted)
    };

    let release = |v: VReg, preg: &mut [Option<PhysReg>], free: &mut Vec<PhysReg>, idx: usize| {
        let vi = v.0 as usize;
        if vi >= preg.len() {
            return;
        }
        if last_use[vi] != idx {
            return;
        }
        if is_param.get(vi).copied().unwrap_or(false) {
            return;
        }
        if let Some(p) = preg[vi].take() {
            if abi::pool_contains(p) {
                free.push(p);
            }
        }
    };

    let get = |v: VReg, preg: &mut [Option<PhysReg>]| -> Result<PhysReg, AllocError> {
        let vi = v.0 as usize;
        preg[vi].ok_or(AllocError::PoolExhausted)
    };

    for (idx, inst) in vinsts.iter().enumerate() {
        if matches!(inst, VInst::Label(..)) {
            continue;
        }

        let uses: Vec<VReg> = inst.uses().collect();
        let defs: Vec<VReg> = inst.defs().collect();

        match inst {
            VInst::Ret { vals, .. } => {
                for (k, v) in vals.iter().enumerate() {
                    let src = get(*v, &mut preg)?;
                    let dst_ret = RET_REGS[k];
                    if src != dst_ret {
                        out.push(PInst::Mv { dst: dst_ret, src });
                    }
                }
                out.push(PInst::Ret);
            }
            VInst::IConst32 { dst, val, .. } => {
                let direct_ret = matches!(
                    vinsts.get(idx + 1),
                    Some(VInst::Ret { vals, .. }) if vals.len() == 1 && vals[0] == *dst
                );
                let p = if direct_ret {
                    RET_REGS[0]
                } else {
                    alloc_reg(&mut free)?
                };
                out.push(PInst::Li { dst: p, imm: *val });
                preg[dst.0 as usize] = Some(p);
            }
            VInst::Mov32 { dst, src, .. } => {
                let s = get(*src, &mut preg)?;
                let p = alloc_reg(&mut free)?;
                if s != p {
                    out.push(PInst::Mv { dst: p, src: s });
                }
                preg[dst.0 as usize] = Some(p);
            }
            VInst::Add32 {
                dst, src1, src2, ..
            } => {
                let a = get(*src1, &mut preg)?;
                let b = get(*src2, &mut preg)?;
                let p = alloc_reg(&mut free)?;
                out.push(PInst::Add {
                    dst: p,
                    src1: a,
                    src2: b,
                });
                preg[dst.0 as usize] = Some(p);
            }
            VInst::Sub32 {
                dst, src1, src2, ..
            } => {
                let a = get(*src1, &mut preg)?;
                let b = get(*src2, &mut preg)?;
                let p = alloc_reg(&mut free)?;
                out.push(PInst::Sub {
                    dst: p,
                    src1: a,
                    src2: b,
                });
                preg[dst.0 as usize] = Some(p);
            }
            VInst::Mul32 {
                dst, src1, src2, ..
            } => {
                let a = get(*src1, &mut preg)?;
                let b = get(*src2, &mut preg)?;
                let p = alloc_reg(&mut free)?;
                out.push(PInst::Mul {
                    dst: p,
                    src1: a,
                    src2: b,
                });
                preg[dst.0 as usize] = Some(p);
            }
            VInst::And32 {
                dst, src1, src2, ..
            } => {
                let a = get(*src1, &mut preg)?;
                let b = get(*src2, &mut preg)?;
                let p = alloc_reg(&mut free)?;
                out.push(PInst::And {
                    dst: p,
                    src1: a,
                    src2: b,
                });
                preg[dst.0 as usize] = Some(p);
            }
            VInst::Or32 {
                dst, src1, src2, ..
            } => {
                let a = get(*src1, &mut preg)?;
                let b = get(*src2, &mut preg)?;
                let p = alloc_reg(&mut free)?;
                out.push(PInst::Or {
                    dst: p,
                    src1: a,
                    src2: b,
                });
                preg[dst.0 as usize] = Some(p);
            }
            VInst::Xor32 {
                dst, src1, src2, ..
            } => {
                let a = get(*src1, &mut preg)?;
                let b = get(*src2, &mut preg)?;
                let p = alloc_reg(&mut free)?;
                out.push(PInst::Xor {
                    dst: p,
                    src1: a,
                    src2: b,
                });
                preg[dst.0 as usize] = Some(p);
            }
            VInst::Neg32 { dst, src, .. } => {
                let s = get(*src, &mut preg)?;
                let p = alloc_reg(&mut free)?;
                out.push(PInst::Neg { dst: p, src: s });
                preg[dst.0 as usize] = Some(p);
            }
            VInst::Bnot32 { dst, src, .. } => {
                let s = get(*src, &mut preg)?;
                let p = alloc_reg(&mut free)?;
                out.push(PInst::Not { dst: p, src: s });
                preg[dst.0 as usize] = Some(p);
            }
            VInst::Shl32 {
                dst, src1, src2, ..
            } => {
                let a = get(*src1, &mut preg)?;
                let b = get(*src2, &mut preg)?;
                let p = alloc_reg(&mut free)?;
                out.push(PInst::Sll {
                    dst: p,
                    src1: a,
                    src2: b,
                });
                preg[dst.0 as usize] = Some(p);
            }
            VInst::ShrS32 {
                dst, src1, src2, ..
            } => {
                let a = get(*src1, &mut preg)?;
                let b = get(*src2, &mut preg)?;
                let p = alloc_reg(&mut free)?;
                out.push(PInst::Sra {
                    dst: p,
                    src1: a,
                    src2: b,
                });
                preg[dst.0 as usize] = Some(p);
            }
            VInst::ShrU32 {
                dst, src1, src2, ..
            } => {
                let a = get(*src1, &mut preg)?;
                let b = get(*src2, &mut preg)?;
                let p = alloc_reg(&mut free)?;
                out.push(PInst::Srl {
                    dst: p,
                    src1: a,
                    src2: b,
                });
                preg[dst.0 as usize] = Some(p);
            }
            VInst::DivS32 { dst, lhs, rhs, .. } => {
                let a = get(*lhs, &mut preg)?;
                let b = get(*rhs, &mut preg)?;
                let p = alloc_reg(&mut free)?;
                out.push(PInst::Div {
                    dst: p,
                    src1: a,
                    src2: b,
                });
                preg[dst.0 as usize] = Some(p);
            }
            VInst::DivU32 { dst, lhs, rhs, .. } => {
                let a = get(*lhs, &mut preg)?;
                let b = get(*rhs, &mut preg)?;
                let p = alloc_reg(&mut free)?;
                out.push(PInst::Divu {
                    dst: p,
                    src1: a,
                    src2: b,
                });
                preg[dst.0 as usize] = Some(p);
            }
            VInst::RemS32 { dst, lhs, rhs, .. } => {
                let a = get(*lhs, &mut preg)?;
                let b = get(*rhs, &mut preg)?;
                let p = alloc_reg(&mut free)?;
                out.push(PInst::Rem {
                    dst: p,
                    src1: a,
                    src2: b,
                });
                preg[dst.0 as usize] = Some(p);
            }
            VInst::RemU32 { dst, lhs, rhs, .. } => {
                let a = get(*lhs, &mut preg)?;
                let b = get(*rhs, &mut preg)?;
                let p = alloc_reg(&mut free)?;
                out.push(PInst::Remu {
                    dst: p,
                    src1: a,
                    src2: b,
                });
                preg[dst.0 as usize] = Some(p);
            }
            VInst::Icmp32 {
                dst,
                lhs,
                rhs,
                cond,
                ..
            } => {
                let l = get(*lhs, &mut preg)?;
                let r = get(*rhs, &mut preg)?;
                let d = alloc_reg(&mut free)?;
                match cond {
                    IcmpCond::Eq => {
                        out.push(PInst::Xor {
                            dst: SCRATCH,
                            src1: l,
                            src2: r,
                        });
                        out.push(PInst::Seqz {
                            dst: d,
                            src: SCRATCH,
                        });
                    }
                    IcmpCond::Ne => {
                        out.push(PInst::Xor {
                            dst: SCRATCH,
                            src1: l,
                            src2: r,
                        });
                        out.push(PInst::Snez {
                            dst: d,
                            src: SCRATCH,
                        });
                    }
                    IcmpCond::LtS => {
                        out.push(PInst::Slt {
                            dst: d,
                            src1: l,
                            src2: r,
                        });
                    }
                    IcmpCond::LeS => {
                        out.push(PInst::Slt {
                            dst: SCRATCH,
                            src1: r,
                            src2: l,
                        });
                        out.push(PInst::Seqz {
                            dst: d,
                            src: SCRATCH,
                        });
                    }
                    IcmpCond::GtS => {
                        out.push(PInst::Slt {
                            dst: d,
                            src1: r,
                            src2: l,
                        });
                    }
                    IcmpCond::GeS => {
                        out.push(PInst::Slt {
                            dst: SCRATCH,
                            src1: l,
                            src2: r,
                        });
                        out.push(PInst::Seqz {
                            dst: d,
                            src: SCRATCH,
                        });
                    }
                    IcmpCond::LtU => {
                        out.push(PInst::Sltu {
                            dst: d,
                            src1: l,
                            src2: r,
                        });
                    }
                    IcmpCond::LeU => {
                        out.push(PInst::Sltu {
                            dst: SCRATCH,
                            src1: r,
                            src2: l,
                        });
                        out.push(PInst::Seqz {
                            dst: d,
                            src: SCRATCH,
                        });
                    }
                    IcmpCond::GtU => {
                        out.push(PInst::Sltu {
                            dst: d,
                            src1: r,
                            src2: l,
                        });
                    }
                    IcmpCond::GeU => {
                        out.push(PInst::Sltu {
                            dst: SCRATCH,
                            src1: l,
                            src2: r,
                        });
                        out.push(PInst::Seqz {
                            dst: d,
                            src: SCRATCH,
                        });
                    }
                }
                preg[dst.0 as usize] = Some(d);
            }
            VInst::IeqImm32 { dst, src, imm, .. } => {
                let s = get(*src, &mut preg)?;
                let d = alloc_reg(&mut free)?;
                out.push(PInst::Li {
                    dst: SCRATCH,
                    imm: *imm,
                });
                out.push(PInst::Xor {
                    dst: SCRATCH,
                    src1: s,
                    src2: SCRATCH,
                });
                out.push(PInst::Seqz {
                    dst: d,
                    src: SCRATCH,
                });
                preg[dst.0 as usize] = Some(d);
            }
            VInst::Load32 {
                dst, base, offset, ..
            } => {
                let b = get(*base, &mut preg)?;
                let p = alloc_reg(&mut free)?;
                out.push(PInst::Lw {
                    dst: p,
                    base: b,
                    offset: *offset,
                });
                preg[dst.0 as usize] = Some(p);
            }
            VInst::Store32 {
                src, base, offset, ..
            } => {
                let s = get(*src, &mut preg)?;
                let b = get(*base, &mut preg)?;
                out.push(PInst::Sw {
                    src: s,
                    base: b,
                    offset: *offset,
                });
            }
            VInst::SlotAddr { dst, slot, .. } => {
                let p = alloc_reg(&mut free)?;
                out.push(PInst::SlotAddr {
                    dst: p,
                    slot: *slot,
                });
                preg[dst.0 as usize] = Some(p);
            }
            VInst::MemcpyWords {
                dst_base,
                src_base,
                size,
                ..
            } => {
                let d = get(*dst_base, &mut preg)?;
                let s = get(*src_base, &mut preg)?;
                out.push(PInst::MemcpyWords {
                    dst: d,
                    src: s,
                    size: *size,
                });
            }
            VInst::Label(..) => {}
            _ => return Err(AllocError::UnsupportedControlFlow),
        }

        for u in uses {
            release(u, &mut preg, &mut free, idx);
        }
        if let VInst::Ret { vals, .. } = inst {
            for v in vals {
                release(*v, &mut preg, &mut free, idx);
            }
        }
        for d in defs {
            release(d, &mut preg, &mut free, idx);
        }
    }

    let mut wrapped = Vec::with_capacity(out.len() + 2);
    wrapped.push(PInst::FrameSetup { spill_slots: 0 });
    wrapped.extend(out);
    wrapped.push(PInst::FrameTeardown { spill_slots: 0 });
    Ok(wrapped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::rv32::abi::func_abi_rv32;
    use crate::vinst::VInst;
    use alloc::string::String;

    #[test]
    fn test_alloc_simple_iconst() {
        let vinsts = vec![
            VInst::IConst32 {
                dst: VReg(0),
                val: 42,
                src_op: None,
            },
            VInst::Ret {
                vals: vec![VReg(0)],
                src_op: None,
            },
        ];
        let func = IrFunction {
            name: String::from("t"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 0,
            return_types: vec![lpir::IrType::I32],
            vreg_types: vec![lpir::IrType::I32],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        };
        let sig = lps_shared::LpsFnSig {
            name: String::from("t"),
            return_type: lps_shared::LpsType::Int,
            parameters: vec![],
        };
        let fa = func_abi_rv32(&sig, func.total_param_slots() as usize);
        let phys = allocate(&vinsts, &fa, &func).unwrap();
        assert!(matches!(phys[0], PInst::FrameSetup { .. }));
        assert!(matches!(phys[1], PInst::Li { dst: 10, imm: 42 }));
        assert!(matches!(phys[2], PInst::Ret));
        assert!(matches!(phys[3], PInst::FrameTeardown { .. }));
    }

    #[test]
    fn test_alloc_add_two_params() {
        let vinsts = vec![
            VInst::Add32 {
                dst: VReg(3),
                src1: VReg(1),
                src2: VReg(2),
                src_op: None,
            },
            VInst::Ret {
                vals: vec![VReg(3)],
                src_op: None,
            },
        ];
        let func = IrFunction {
            name: String::from("add_two_ints"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 2,
            return_types: vec![lpir::IrType::I32],
            vreg_types: vec![
                lpir::IrType::I32,
                lpir::IrType::I32,
                lpir::IrType::I32,
                lpir::IrType::I32,
            ],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        };
        let sig = lps_shared::LpsFnSig {
            name: String::from("add_two_ints"),
            return_type: lps_shared::LpsType::Int,
            parameters: vec![
                lps_shared::FnParam {
                    name: String::from("a"),
                    ty: lps_shared::LpsType::Int,
                    qualifier: lps_shared::ParamQualifier::In,
                },
                lps_shared::FnParam {
                    name: String::from("b"),
                    ty: lps_shared::LpsType::Int,
                    qualifier: lps_shared::ParamQualifier::In,
                },
            ],
        };
        let fa = func_abi_rv32(&sig, func.total_param_slots() as usize);
        let phys = allocate(&vinsts, &fa, &func).unwrap();
        assert!(matches!(phys[0], PInst::FrameSetup { .. }));
        assert!(matches!(phys[phys.len() - 2], PInst::Ret));
        assert!(matches!(phys[phys.len() - 1], PInst::FrameTeardown { .. }));
    }

    #[test]
    fn test_alloc_error_on_branch() {
        let vinsts = vec![
            VInst::Br {
                target: 1,
                src_op: None,
            },
            VInst::Label(1, None),
            VInst::Ret {
                vals: vec![VReg(0)],
                src_op: None,
            },
        ];
        let func = IrFunction {
            name: String::from("t"),
            is_entry: true,
            vmctx_vreg: VReg(0),
            param_count: 0,
            return_types: vec![],
            vreg_types: vec![lpir::IrType::I32],
            slots: vec![],
            body: vec![],
            vreg_pool: vec![],
        };
        let sig = lps_shared::LpsFnSig {
            name: String::from("t"),
            return_type: lps_shared::LpsType::Void,
            parameters: vec![],
        };
        let fa = func_abi_rv32(&sig, func.total_param_slots() as usize);
        let result = allocate(&vinsts, &fa, &func);
        assert!(matches!(result, Err(AllocError::UnsupportedControlFlow)));
    }
}
