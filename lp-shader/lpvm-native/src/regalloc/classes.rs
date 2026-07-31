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

use crate::abi::RegClass;
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
///
/// Built by [`build_operand_layout`](crate::regalloc::walk) rather than by a
/// pass of its own. That pass already visits every def of every instruction, and
/// `VInst::for_each_def` is generic over its callback — a second walk would mean
/// a second monomorphization of a match over the whole instruction set, which is
/// real flash in a compiler that ships on the device.
#[derive(Debug, Clone, Default)]
pub struct VRegClasses {
    /// Indexed by vreg; short of the true vreg count whenever the tail is all
    /// integer, which [`VRegClasses::of`] handles.
    classes: Vec<RegClass>,
}

impl VRegClasses {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `vreg`'s defining instruction puts it in `class`.
    ///
    /// [`RegClass::Int`] records **nothing**: it is already the answer for any
    /// vreg the table does not cover, and a vreg is defined once, so there is no
    /// earlier non-integer entry that would need overwriting.
    ///
    /// That is not just tidiness. It makes the integer path provably empty, so
    /// on a build where the class is a compile-time integer the optimizer
    /// deletes this call and the table with it — which is what keeps a
    /// class-aware allocator from charging the Q32-only firmware for a
    /// capability it does not use.
    pub fn record_def(&mut self, vreg: VReg, class: RegClass) {
        if class == RegClass::Int {
            return;
        }
        let idx = vreg.0 as usize;
        if idx >= self.classes.len() {
            self.classes.resize(idx + 1, RegClass::Int);
        }
        self.classes[idx] = class;
    }

    /// Class of `vreg`; [`RegClass::Int`] for anything outside the table.
    ///
    /// Out of range means "never recorded as anything but integer" — either a
    /// vreg with no def in the stream (an entry parameter or the vmctx pointer,
    /// integer on every ABI we target) or one past the recorded tail.
    pub fn of(&self, vreg: VReg) -> RegClass {
        self.classes
            .get(vreg.0 as usize)
            .copied()
            .unwrap_or(RegClass::Int)
    }

    /// Derive every vreg's class by walking the instruction stream.
    ///
    /// Test-only: production builds fill the table from
    /// [`build_operand_layout`](crate::regalloc::walk)'s existing walk, and this
    /// exists so class derivation can be exercised on its own.
    #[cfg(test)]
    pub fn compute(vinsts: &[VInst], vreg_pool: &[VReg], func_abi: &crate::abi::FuncAbi) -> Self {
        let _ = func_abi;
        let mut classes = Self::new();
        for inst in vinsts {
            let mut def_idx = 0usize;
            inst.for_each_def(vreg_pool, |def_vreg| {
                classes.record_def(def_vreg, def_class(inst, def_idx));
                def_idx += 1;
            });
        }
        classes
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
        let classes = VRegClasses::compute(&insts, &[], &abi_fixtures::void_func_abi());
        for v in 0..4u16 {
            assert_eq!(classes.of(VReg(v)), RegClass::Int);
        }
    }

    #[test]
    fn unknown_vregs_answer_int() {
        let classes = VRegClasses::compute(&[], &[], &abi_fixtures::void_func_abi());
        assert_eq!(classes.of(VReg(9999)), RegClass::Int);
    }

    /// An all-integer function must not allocate a class table at all — that is
    /// what keeps the class-aware allocator free for the Q32 path.
    #[test]
    fn an_all_integer_function_allocates_no_table() {
        let insts = vinsts();
        let classes = VRegClasses::compute(&insts, &[], &abi_fixtures::void_func_abi());
        assert_eq!(classes.classes.capacity(), 0);
    }

    #[test]
    fn recording_a_float_def_grows_the_table_and_reads_back() {
        let mut classes = VRegClasses::new();
        classes.record_def(VReg(3), RegClass::Float);
        assert_eq!(classes.of(VReg(3)), RegClass::Float);
        // Everything the growth skipped over stays integer.
        for v in [0u16, 1, 2, 4, 500] {
            assert_eq!(classes.of(VReg(v)), RegClass::Int);
        }
    }

    /// Recording integer is a no-op, which is what makes the whole table
    /// disappear on an all-integer function.
    #[test]
    fn recording_an_integer_def_allocates_nothing() {
        let mut classes = VRegClasses::new();
        classes.record_def(VReg(9), RegClass::Int);
        assert_eq!(classes.classes.capacity(), 0);
        assert_eq!(classes.of(VReg(9)), RegClass::Int);
    }

    #[test]
    fn operand_classes_match_the_defining_instruction() {
        let insts = vinsts();
        let classes = VRegClasses::compute(&insts, &[], &abi_fixtures::void_func_abi());
        assert_eq!(def_class(&insts[1], 0), classes.of(VReg(1)));
        assert_eq!(use_class(&insts[1], 0), classes.of(VReg(0)));
    }
}
