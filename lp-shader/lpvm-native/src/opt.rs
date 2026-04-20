//! Post-lowering VInst optimizations.
//!
//! Folds `IConst32` + `AluRRR` patterns into `AluRRI` when the constant fits
//! an RV32I 12-bit signed immediate. Does not change instruction count, so
//! region tree indices remain valid. Dead `IConst32` defs are eliminated by
//! the register allocator.

use alloc::vec::Vec;

use crate::imm::fits_imm12;
use crate::lower::LoweredFunction;
use crate::vinst::{AluImmOp, AluOp, VInst, VReg};

fn alu_to_imm_op(op: AluOp) -> Option<AluImmOp> {
    match op {
        AluOp::Add => Some(AluImmOp::Addi),
        AluOp::And => Some(AluImmOp::Andi),
        AluOp::Or => Some(AluImmOp::Ori),
        AluOp::Xor => Some(AluImmOp::Xori),
        AluOp::Sll => Some(AluImmOp::Slli),
        AluOp::SrlU => Some(AluImmOp::SrliU),
        AluOp::SraS => Some(AluImmOp::SraiS),
        _ => None,
    }
}

fn is_commutative(op: AluOp) -> bool {
    matches!(op, AluOp::Add | AluOp::And | AluOp::Or | AluOp::Xor)
}

/// Fold `IConst32` + `AluRRR` → `AluRRI` where the constant fits imm12.
///
/// Also handles `Sub` with a constant rhs by converting to `Addi` with
/// the negated immediate (when the negated value fits imm12).
///
/// The pass is *loop-aware*: it walks `vinsts` linearly, but a recorded
/// `IConst32` value cannot be used to fold a use that lives inside a loop
/// where the same vreg is also redefined. Without this guard the linear
/// walk would happily fold the first iteration's value and leave every
/// subsequent iteration broken (e.g. `px_off` initialised to 0 outside a
/// per-pixel loop and `+= 8` inside it would still resolve to `+ 0` at the
/// in-loop use, causing every pixel to overwrite pixel 0).
pub fn fold_immediates(lowered: &mut LoweredFunction) {
    let max_vreg = max_vreg_index(&lowered.vinsts, &lowered.vreg_pool);
    if max_vreg == 0 {
        return;
    }
    let mut vreg_const: Vec<Option<i32>> = Vec::new();
    vreg_const.resize(max_vreg + 1, None);

    let loop_defs = compute_loop_def_sets(lowered, max_vreg);
    let inside_loop_with_def = |i: usize, v: VReg| -> bool {
        let vidx = v.0 as usize;
        loop_defs.iter().any(|ld| {
            i >= ld.header_idx && i <= ld.backedge_idx && vidx < ld.defs.len() && ld.defs[vidx]
        })
    };

    for i in 0..lowered.vinsts.len() {
        match &lowered.vinsts[i] {
            VInst::IConst32 { dst, val, .. } => {
                let idx = dst.0 as usize;
                if idx < vreg_const.len() {
                    vreg_const[idx] = Some(*val);
                }
            }
            VInst::AluRRR {
                op,
                dst,
                src1,
                src2,
                src_op,
            } => {
                let op = *op;
                let dst = *dst;
                let src1 = *src1;
                let src2 = *src2;
                let src_op = *src_op;

                let c2 = vreg_const
                    .get(src2.0 as usize)
                    .copied()
                    .flatten()
                    .filter(|_| !inside_loop_with_def(i, src2));
                let c1 = vreg_const
                    .get(src1.0 as usize)
                    .copied()
                    .flatten()
                    .filter(|_| !inside_loop_with_def(i, src1));

                if op == AluOp::Sub {
                    if let Some(val) = c2 {
                        let neg = val.wrapping_neg();
                        if fits_imm12(neg) {
                            lowered.vinsts[i] = VInst::AluRRI {
                                op: AluImmOp::Addi,
                                dst,
                                src: src1,
                                imm: neg,
                                src_op,
                            };
                        }
                    }
                } else if let Some(imm_op) = alu_to_imm_op(op) {
                    if let Some(val) = c2 {
                        if fits_imm12(val) {
                            lowered.vinsts[i] = VInst::AluRRI {
                                op: imm_op,
                                dst,
                                src: src1,
                                imm: val,
                                src_op,
                            };
                        }
                    } else if is_commutative(op) {
                        if let Some(val) = c1 {
                            if fits_imm12(val) {
                                lowered.vinsts[i] = VInst::AluRRI {
                                    op: imm_op,
                                    dst,
                                    src: src2,
                                    imm: val,
                                    src_op,
                                };
                            }
                        }
                    }
                }

                let idx = dst.0 as usize;
                if idx < vreg_const.len() {
                    vreg_const[idx] = None;
                }
            }
            other => {
                other.for_each_def(&lowered.vreg_pool, |v| {
                    let idx = v.0 as usize;
                    if idx < vreg_const.len() {
                        vreg_const[idx] = None;
                    }
                });
            }
        }
    }
}

/// Per-loop bitset of vregs def'd anywhere inside that loop's VInst range.
struct LoopDefSet {
    header_idx: usize,
    backedge_idx: usize,
    /// `defs[v.0 as usize]` is true iff `v` is def'd at least once in
    /// `vinsts[header_idx..=backedge_idx]`.
    defs: Vec<bool>,
}

fn compute_loop_def_sets(lowered: &LoweredFunction, max_vreg: usize) -> Vec<LoopDefSet> {
    let mut out = Vec::with_capacity(lowered.loop_regions.len());
    for lr in &lowered.loop_regions {
        let mut defs = Vec::new();
        defs.resize(max_vreg + 1, false);
        let end = lr.backedge_idx.min(lowered.vinsts.len().saturating_sub(1));
        if lr.header_idx <= end {
            for inst in &lowered.vinsts[lr.header_idx..=end] {
                inst.for_each_def(&lowered.vreg_pool, |v| {
                    let idx = v.0 as usize;
                    if idx < defs.len() {
                        defs[idx] = true;
                    }
                });
            }
        }
        out.push(LoopDefSet {
            header_idx: lr.header_idx,
            backedge_idx: lr.backedge_idx,
            defs,
        });
    }
    out
}

fn max_vreg_index(vinsts: &[VInst], pool: &[VReg]) -> usize {
    let mut max = 0usize;
    for inst in vinsts {
        inst.for_each_vreg_touching(pool, |v| {
            let idx = v.0 as usize;
            if idx > max {
                max = idx;
            }
        });
    }
    max
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::region::{Region, RegionTree};
    use crate::vinst::{AluImmOp, AluOp, ModuleSymbols, SRC_OP_NONE, VInst, VReg};

    fn make_lowered(vinsts: Vec<VInst>) -> LoweredFunction {
        let len = vinsts.len() as u16;
        let mut tree = RegionTree::new();
        if len > 0 {
            let root = tree.push(Region::Linear { start: 0, end: len });
            tree.root = root;
        }
        LoweredFunction {
            vinsts,
            vreg_pool: vec![],
            symbols: ModuleSymbols::default(),
            loop_regions: vec![],
            region_tree: tree,
            lpir_slots: vec![],
        }
    }

    #[test]
    fn fold_add_imm12() {
        let mut lowered = make_lowered(vec![
            VInst::IConst32 {
                dst: VReg(1),
                val: 42,
                src_op: SRC_OP_NONE,
            },
            VInst::AluRRR {
                op: AluOp::Add,
                dst: VReg(2),
                src1: VReg(0),
                src2: VReg(1),
                src_op: SRC_OP_NONE,
            },
        ]);
        fold_immediates(&mut lowered);
        assert!(matches!(lowered.vinsts[0], VInst::IConst32 { .. }));
        assert!(matches!(
            lowered.vinsts[1],
            VInst::AluRRI {
                op: AluImmOp::Addi,
                imm: 42,
                ..
            }
        ));
    }

    #[test]
    fn no_fold_large_constant() {
        let mut lowered = make_lowered(vec![
            VInst::IConst32 {
                dst: VReg(1),
                val: 3000,
                src_op: SRC_OP_NONE,
            },
            VInst::AluRRR {
                op: AluOp::Add,
                dst: VReg(2),
                src1: VReg(0),
                src2: VReg(1),
                src_op: SRC_OP_NONE,
            },
        ]);
        fold_immediates(&mut lowered);
        assert!(matches!(
            lowered.vinsts[1],
            VInst::AluRRR { op: AluOp::Add, .. }
        ));
    }

    #[test]
    fn fold_commutative_src1() {
        let mut lowered = make_lowered(vec![
            VInst::IConst32 {
                dst: VReg(0),
                val: 7,
                src_op: SRC_OP_NONE,
            },
            VInst::AluRRR {
                op: AluOp::Add,
                dst: VReg(2),
                src1: VReg(0),
                src2: VReg(1),
                src_op: SRC_OP_NONE,
            },
        ]);
        fold_immediates(&mut lowered);
        match &lowered.vinsts[1] {
            VInst::AluRRI {
                op: AluImmOp::Addi,
                src,
                imm: 7,
                ..
            } => {
                assert_eq!(*src, VReg(1));
            }
            other => panic!("expected AluRRI, got {other:?}"),
        }
    }

    #[test]
    fn fold_sub_to_addi_neg() {
        let mut lowered = make_lowered(vec![
            VInst::IConst32 {
                dst: VReg(1),
                val: 10,
                src_op: SRC_OP_NONE,
            },
            VInst::AluRRR {
                op: AluOp::Sub,
                dst: VReg(2),
                src1: VReg(0),
                src2: VReg(1),
                src_op: SRC_OP_NONE,
            },
        ]);
        fold_immediates(&mut lowered);
        assert!(matches!(
            lowered.vinsts[1],
            VInst::AluRRI {
                op: AluImmOp::Addi,
                imm: -10,
                ..
            }
        ));
    }

    #[test]
    fn fold_shift_rhs_only() {
        let mut lowered = make_lowered(vec![
            VInst::IConst32 {
                dst: VReg(1),
                val: 4,
                src_op: SRC_OP_NONE,
            },
            VInst::AluRRR {
                op: AluOp::Sll,
                dst: VReg(2),
                src1: VReg(0),
                src2: VReg(1),
                src_op: SRC_OP_NONE,
            },
        ]);
        fold_immediates(&mut lowered);
        assert!(matches!(
            lowered.vinsts[1],
            VInst::AluRRI {
                op: AluImmOp::Slli,
                imm: 4,
                ..
            }
        ));
    }

    #[test]
    fn no_fold_shift_lhs() {
        let mut lowered = make_lowered(vec![
            VInst::IConst32 {
                dst: VReg(0),
                val: 4,
                src_op: SRC_OP_NONE,
            },
            VInst::AluRRR {
                op: AluOp::Sll,
                dst: VReg(2),
                src1: VReg(0),
                src2: VReg(1),
                src_op: SRC_OP_NONE,
            },
        ]);
        fold_immediates(&mut lowered);
        assert!(matches!(
            lowered.vinsts[1],
            VInst::AluRRR { op: AluOp::Sll, .. }
        ));
    }

    #[test]
    fn no_fold_mul() {
        let mut lowered = make_lowered(vec![
            VInst::IConst32 {
                dst: VReg(1),
                val: 2,
                src_op: SRC_OP_NONE,
            },
            VInst::AluRRR {
                op: AluOp::Mul,
                dst: VReg(2),
                src1: VReg(0),
                src2: VReg(1),
                src_op: SRC_OP_NONE,
            },
        ]);
        fold_immediates(&mut lowered);
        assert!(matches!(
            lowered.vinsts[1],
            VInst::AluRRR { op: AluOp::Mul, .. }
        ));
    }

    /// Regression: `IConst32` defined *before* a loop must not be folded
    /// into uses *inside* the loop when the same vreg is also redefined
    /// inside the loop. Without the loop-aware guard the linear walk
    /// would happily fold the first iteration's value (e.g. `px_off=0`)
    /// and break every subsequent iteration. This was the root cause of
    /// the M4a per-pixel-loop regression where only pixel 0 was written.
    #[test]
    fn no_fold_loop_invariant_constant_when_vreg_mutated_in_loop() {
        use crate::lower::LoopRegion;

        // VInsts:
        //   0: IConst32 v1 = 0       (px_off init, BEFORE loop)
        //   1: Label(L_header)       (loop header, idx=1)
        //   2: AluRRR v3 = v0 + v1   (tex_ptr + px_off; would fold to +0!)
        //   3: AluRRI v1 = v1 + 8    (px_off += 8)
        //   4: Br L_header           (back-edge, idx=4)
        let mut lowered = make_lowered(vec![
            VInst::IConst32 {
                dst: VReg(1),
                val: 0,
                src_op: SRC_OP_NONE,
            },
            VInst::Label(0, SRC_OP_NONE),
            VInst::AluRRR {
                op: AluOp::Add,
                dst: VReg(3),
                src1: VReg(0),
                src2: VReg(1),
                src_op: SRC_OP_NONE,
            },
            VInst::AluRRI {
                op: AluImmOp::Addi,
                dst: VReg(1),
                src: VReg(1),
                imm: 8,
                src_op: SRC_OP_NONE,
            },
            VInst::Br {
                target: 0,
                src_op: SRC_OP_NONE,
            },
        ]);
        lowered.loop_regions.push(LoopRegion {
            header_idx: 1,
            backedge_idx: 4,
        });
        fold_immediates(&mut lowered);
        assert!(
            matches!(lowered.vinsts[2], VInst::AluRRR { op: AluOp::Add, .. }),
            "in-loop use of mutated vreg must not fold to addi+0; got {:?}",
            lowered.vinsts[2]
        );
    }

    #[test]
    fn vreg_redef_clears_constant() {
        let mut lowered = make_lowered(vec![
            VInst::IConst32 {
                dst: VReg(1),
                val: 5,
                src_op: SRC_OP_NONE,
            },
            VInst::Mov {
                dst: VReg(1),
                src: VReg(3),
                src_op: SRC_OP_NONE,
            },
            VInst::AluRRR {
                op: AluOp::Add,
                dst: VReg(2),
                src1: VReg(0),
                src2: VReg(1),
                src_op: SRC_OP_NONE,
            },
        ]);
        fold_immediates(&mut lowered);
        assert!(matches!(
            lowered.vinsts[2],
            VInst::AluRRR { op: AluOp::Add, .. }
        ));
    }
}
