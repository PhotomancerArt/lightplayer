//! Register class of each virtual register and each instruction operand.
//!
//! The allocator runs one pool per [`RegClass`], so every allocation decision
//! needs to know which class it is deciding for. This module is the single
//! place that answers.
//!
//! # Why the class comes off the instruction, not off the LPIR type
//!
//! The obvious source would be the vreg's LPIR type — `IrType::F32` → float.
//! It is the wrong one. In Q16.16 mode a GLSL `float` **is an integer**: it is
//! a fixed-point value that lives in a GPR and is added with `ADD`. The same
//! LPIR `F32` value in native-f32 mode lives in an FPR and is added with a
//! hardware FPU instruction. Class is therefore a function of
//! `(type, float_mode)`, never of type alone.
//!
//! Lowering already evaluates exactly that pair when it picks which [`VInst`]
//! to emit. Reading the class back off the chosen instruction reuses that
//! decision instead of re-deriving it, which is why the allocator never has to
//! know which float mode it is running under.
//!
//! Every instruction the backend emits today is integer, so every operand is
//! [`RegClass::Int`] — including every `f32` in a Q32 shader, which is the
//! correct answer and not a placeholder. The float arms arrive with the float
//! VInsts.

use alloc::vec::Vec;

use crate::abi::{FuncAbi, RegClass};
use crate::vinst::{VInst, VReg};

/// Register class of the `def_idx`-th value `inst` defines.
///
/// `def_idx` counts defs in [`VInst::for_each_def`] order. It is a parameter
/// rather than an instruction-wide answer because a single call can return
/// values of mixed class once float returns exist.
pub fn def_class(inst: &VInst, def_idx: usize) -> RegClass {
    let _ = (inst, def_idx);
    RegClass::Int
}

/// Register class the `use_idx`-th operand of `inst` must be supplied in.
///
/// `use_idx` counts uses in [`VInst::for_each_use`] order.
pub fn use_class(inst: &VInst, use_idx: usize) -> RegClass {
    let _ = (inst, use_idx);
    RegClass::Int
}

/// The register class of every virtual register in one function.
///
/// A vreg has exactly one class for its whole life: it is defined once, and the
/// defining instruction fixes the register file the value lives in. Uses must
/// agree, which [`crate::regalloc::verify`] checks.
#[derive(Debug, Clone)]
pub struct VRegClasses {
    classes: Vec<RegClass>,
}

impl VRegClasses {
    /// Derive every vreg's class from the instruction that defines it.
    ///
    /// `num_vregs` bounds the table; anything past it, and any vreg with no def
    /// in the stream, answers [`RegClass::Int`]. Undefined vregs are entry
    /// parameters and the vmctx pointer, which are integer on every ABI we
    /// target — a hard-float ABI's float parameters get their class from
    /// `func_abi` when that lands.
    pub fn compute(
        vinsts: &[VInst],
        vreg_pool: &[VReg],
        func_abi: &FuncAbi,
        num_vregs: usize,
    ) -> Self {
        let _ = func_abi;
        let mut classes = vec![RegClass::Int; num_vregs];
        for inst in vinsts {
            let mut def_idx = 0usize;
            inst.for_each_def(vreg_pool, |def_vreg| {
                let class = def_class(inst, def_idx);
                if let Some(slot) = classes.get_mut(def_vreg.0 as usize) {
                    *slot = class;
                }
                def_idx += 1;
            });
        }
        Self { classes }
    }

    /// Class of `vreg`; [`RegClass::Int`] for anything outside the table.
    pub fn of(&self, vreg: VReg) -> RegClass {
        self.classes
            .get(vreg.0 as usize)
            .copied()
            .unwrap_or(RegClass::Int)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regalloc::test::abi_fixtures;
    use crate::vinst::{AluOp, SRC_OP_NONE};

    fn vinsts() -> Vec<VInst> {
        vec![
            VInst::IConst32 {
                dst: VReg(0),
                val: 1,
                src_op: SRC_OP_NONE,
            },
            VInst::AluRRR {
                op: AluOp::Add,
                dst: VReg(1),
                src1: VReg(0),
                src2: VReg(0),
                src_op: SRC_OP_NONE,
            },
        ]
    }

    /// Q32 is an integer world: every vreg, including every GLSL `float`, is
    /// integer-class. This is the invariant the f32 milestones must not break
    /// for the fixed-point path.
    #[test]
    fn every_vreg_is_integer_class_today() {
        let insts = vinsts();
        let classes = VRegClasses::compute(&insts, &[], &abi_fixtures::void_func_abi(), 4);
        for v in 0..4u16 {
            assert_eq!(classes.of(VReg(v)), RegClass::Int);
        }
    }

    #[test]
    fn unknown_vregs_answer_int() {
        let classes = VRegClasses::compute(&[], &[], &abi_fixtures::void_func_abi(), 0);
        assert_eq!(classes.of(VReg(9999)), RegClass::Int);
    }

    #[test]
    fn operand_classes_match_the_defining_instruction() {
        let insts = vinsts();
        let classes = VRegClasses::compute(&insts, &[], &abi_fixtures::void_func_abi(), 4);
        assert_eq!(def_class(&insts[1], 0), classes.of(VReg(1)));
        assert_eq!(use_class(&insts[1], 0), classes.of(VReg(0)));
    }
}
